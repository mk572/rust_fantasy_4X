//! The battle agent's network (`docs/learning-ai.md` 7.2 and 7.3), in Burn.
//!
//! One shared trunk reads the field picture, the unit tokens, the spell tokens and the globals
//! into a context vector. The heads on top give logits over the fixed vocabularies of `act.rs`:
//! the unit's order (cells by a linear layer, enemy units by a pointer over the unit tokens),
//! the wizard's choice, a spell's aim and its mana, and a value. The scripted AI's advice enters
//! as a bonus to the logit of the order it would give, scaled by a learned gain.

use burn::module::Param;
use burn::nn::{Linear, LinearConfig};
use burn::prelude::*;
use burn::tensor::activation::{log_softmax, relu};

use crate::act::{AIM_VOCAB, CELLS, MANA_VOCAB, UNIT_VOCAB, WIZARD_VOCAB};
use crate::obs::{
    Observation, FLEE_VALUE, GLOBALS, GRID_CHANNELS, MAX_SPELLS, MAX_UNITS, SPELL_FEATURES,
};

const H: usize = 128;
const HS: usize = 64;
const CTX: usize = 256;
const NEG: f32 = 1.0e9;

#[derive(Module, Debug)]
pub struct PolicyNet {
    grid_lin: Linear,
    unit_lin: Linear,
    spell_lin: Linear,
    glob_lin: Linear,
    ctx_lin: Linear,
    head_lin: Linear,
    move_lin: Linear,
    wall_a_lin: Linear,
    wall_s_lin: Linear,
    done_lin: Linear,
    att1: Linear,
    att2: Linear,
    sho1: Linear,
    sho2: Linear,
    val1: Linear,
    val2: Linear,
    wiz_lin: Linear,
    aim_hid: Linear,
    aim_cell: Linear,
    aim_none: Linear,
    ptr1: Linear,
    ptr2: Linear,
    mana_lin: Linear,
    adv_gain: Param<Tensor<1>>,
    /// How much the wizard's choice to flee follows what fleeing is worth against what the
    /// critic expects of fighting on.
    flee_gain: Param<Tensor<1>>,
}

/// A batch of observations as tensors.
pub struct Inputs {
    pub n: usize,
    grid: Tensor<2>,
    units: Tensor<3>,
    unit_mask: Tensor<2>,
    spells: Tensor<3>,
    spell_mask: Tensor<2>,
    globals: Tensor<2>,
}

/// What the trunk computed for a batch.
pub struct Trunk {
    ctx: Tensor<2>,
    tokens: Tensor<3>,
    spell_tokens: Tensor<3>,
    /// What fleeing now would score, `[n, 1]` (a number of the globals).
    flee_value: Tensor<2>,
}

fn f32s(v: Vec<f32>, shape: impl Into<burn::tensor::Shape>, device: &Device) -> TensorData {
    let _ = device;
    TensorData::new(v, shape)
}

impl Inputs {
    pub fn new(obs: &[&Observation], unit_features: usize, device: &Device) -> Self {
        let n = obs.len();
        let cat = |f: &dyn Fn(&Observation) -> Vec<f32>| -> Vec<f32> {
            obs.iter().flat_map(|o| f(o)).collect()
        };
        let mask = |m: &[bool]| -> Vec<f32> { m.iter().map(|&b| f32::from(b)).collect() };
        Self {
            n,
            grid: Tensor::from_data(
                f32s(cat(&|o| o.grid.clone()), [n, GRID_CHANNELS * CELLS], device),
                device,
            ),
            units: Tensor::from_data(
                f32s(
                    cat(&|o| o.units.clone()),
                    [n, MAX_UNITS, unit_features],
                    device,
                ),
                device,
            ),
            unit_mask: Tensor::from_data(
                f32s(cat(&|o| mask(&o.unit_mask)), [n, MAX_UNITS], device),
                device,
            ),
            spells: Tensor::from_data(
                f32s(
                    cat(&|o| o.spells.clone()),
                    [n, MAX_SPELLS, SPELL_FEATURES],
                    device,
                ),
                device,
            ),
            spell_mask: Tensor::from_data(
                f32s(cat(&|o| mask(&o.spell_mask)), [n, MAX_SPELLS], device),
                device,
            ),
            globals: Tensor::from_data(
                f32s(cat(&|o| o.globals.clone()), [n, GLOBALS], device),
                device,
            ),
        }
    }
}

/// Add a large negative number where the mask is 0.
pub fn masked(logits: Tensor<2>, mask: Tensor<2>) -> Tensor<2> {
    logits + mask.sub_scalar(1.0).mul_scalar(NEG)
}

impl PolicyNet {
    /// A small network that stands in while the real one is moved through the optimizer.
    pub fn placeholder() -> Self {
        Self::new(1, &Device::flex())
    }

    pub fn new(unit_features: usize, device: &Device) -> Self {
        let lin = |i: usize, o: usize| LinearConfig::new(i, o).init(device);
        Self {
            grid_lin: lin(GRID_CHANNELS * CELLS, H),
            unit_lin: lin(unit_features, H),
            spell_lin: lin(SPELL_FEATURES, HS),
            glob_lin: lin(GLOBALS, 32),
            ctx_lin: lin(H + H + HS + 32, CTX),
            head_lin: lin(CTX + H, 256),
            move_lin: lin(256, CELLS),
            wall_a_lin: lin(256, CELLS),
            wall_s_lin: lin(256, CELLS),
            done_lin: lin(256, 1),
            att1: lin(H + H, 64),
            att2: lin(64, 1),
            sho1: lin(H + H, 64),
            sho2: lin(64, 1),
            val1: lin(CTX, 128),
            val2: lin(128, 1),
            wiz_lin: lin(CTX, WIZARD_VOCAB),
            aim_hid: lin(CTX + HS, 256),
            aim_cell: lin(256, CELLS),
            aim_none: lin(256, 1),
            ptr1: lin(HS + H, 64),
            ptr2: lin(64, 1),
            mana_lin: lin(CTX + HS, MANA_VOCAB),
            adv_gain: Param::from_tensor(Tensor::from_floats([2.0f32], device)),
            flee_gain: Param::from_tensor(Tensor::from_floats([12.0f32], device)),
        }
    }

    /// The context of each observation.
    pub fn trunk(&self, x: &Inputs) -> Trunk {
        let g = relu(self.grid_lin.forward(x.grid.clone()));
        let tokens = relu(self.unit_lin.forward(x.units.clone()));
        let m3 = x.unit_mask.clone().unsqueeze_dim::<3>(2);
        let count = x.unit_mask.clone().sum_dim(1).clamp_min(1.0);
        let pooled = (tokens.clone() * m3).sum_dim(1).squeeze_dim::<2>(1) / count;
        let spell_tokens = relu(self.spell_lin.forward(x.spells.clone()));
        let s3 = x.spell_mask.clone().unsqueeze_dim::<3>(2);
        let s_count = x.spell_mask.clone().sum_dim(1).clamp_min(1.0);
        let s_pooled = (spell_tokens.clone() * s3).sum_dim(1).squeeze_dim::<2>(1) / s_count;
        let gl = relu(self.glob_lin.forward(x.globals.clone()));
        let ctx = relu(
            self.ctx_lin
                .forward(Tensor::cat(vec![g, pooled, s_pooled, gl], 1)),
        );
        let n = x.n;
        let flee_value = x.globals.clone().slice([0..n, FLEE_VALUE..FLEE_VALUE + 1]);
        Trunk {
            ctx,
            tokens,
            spell_tokens,
            flee_value,
        }
    }

    pub fn value(&self, t: &Trunk) -> Tensor<1> {
        let v = self.val2.forward(relu(self.val1.forward(t.ctx.clone())));
        v.squeeze_dim::<1>(1)
    }

    /// Pointer logits over the unit tokens for each row: `[n, MAX_UNITS]`.
    fn pointer(from: Tensor<2>, tokens: Tensor<3>, l1: &Linear, l2: &Linear) -> Tensor<2> {
        let rows = from.clone().unsqueeze_dim::<3>(1).repeat_dim(1, MAX_UNITS);
        let joined = Tensor::cat(vec![rows, tokens], 2);
        l2.forward(relu(l1.forward(joined))).squeeze_dim::<2>(2)
    }

    /// The embedding of the unit token a one-hot picks: `[n, width]`.
    fn pick(tokens: Tensor<3>, one_hot: Tensor<2>) -> Tensor<2> {
        (tokens * one_hot.unsqueeze_dim::<3>(2))
            .sum_dim(1)
            .squeeze_dim::<2>(1)
    }

    /// Logits over the unit vocabulary for the units `actor` (a one-hot over the tokens) picks,
    /// with the scripted AI's advice (a one-hot over the vocabulary, or zeros).
    pub fn unit_logits(&self, t: &Trunk, actor: Tensor<2>, advice: Tensor<2>) -> Tensor<2> {
        let h_a = Self::pick(t.tokens.clone(), actor);
        let z = relu(
            self.head_lin
                .forward(Tensor::cat(vec![t.ctx.clone(), h_a.clone()], 1)),
        );
        let attack = Self::pointer(h_a.clone(), t.tokens.clone(), &self.att1, &self.att2);
        let shoot = Self::pointer(h_a, t.tokens.clone(), &self.sho1, &self.sho2);
        let logits = Tensor::cat(
            vec![
                self.move_lin.forward(z.clone()),
                attack,
                shoot,
                self.wall_a_lin.forward(z.clone()),
                self.wall_s_lin.forward(z.clone()),
                self.done_lin.forward(z),
            ],
            1,
        );
        debug_assert_eq!(logits.dims()[1], UNIT_VOCAB);
        let gain = self.adv_gain.val().unsqueeze_dim::<2>(0);
        logits + advice * gain
    }

    /// Logits of the wizard's choice: skip, flee, or a spell slot.
    pub fn wizard_logits(&self, t: &Trunk) -> Tensor<2> {
        let logits = self.wiz_lin.forward(t.ctx.clone());
        // Fleeing is the more likely the more it scores above what the critic expects.
        let edge = t.flee_value.clone() - self.value(t).detach().unsqueeze_dim::<2>(1);
        let gain = self.flee_gain.val().unsqueeze_dim::<2>(0);
        let mut at_flee = vec![0.0f32; WIZARD_VOCAB];
        at_flee[crate::act::FLEE] = 1.0;
        let at_flee = Tensor::<2>::from_data(
            TensorData::new(at_flee, [1, WIZARD_VOCAB]),
            &logits.device(),
        );
        logits + edge * gain * at_flee
    }

    /// Logits of a spell's aim, given a one-hot over the spell slots.
    pub fn aim_logits(&self, t: &Trunk, spell: Tensor<2>) -> Tensor<2> {
        let e = (t.spell_tokens.clone() * spell.unsqueeze_dim::<3>(2))
            .sum_dim(1)
            .squeeze_dim::<2>(1);
        let z = relu(
            self.aim_hid
                .forward(Tensor::cat(vec![t.ctx.clone(), e.clone()], 1)),
        );
        let ptr = Self::pointer(e, t.tokens.clone(), &self.ptr1, &self.ptr2);
        let logits = Tensor::cat(
            vec![
                self.aim_none.forward(z.clone()),
                ptr,
                self.aim_cell.forward(z),
            ],
            1,
        );
        debug_assert_eq!(logits.dims()[1], AIM_VOCAB);
        logits
    }

    /// Logits of the mana bin of a spell.
    pub fn mana_logits(&self, t: &Trunk, spell: Tensor<2>) -> Tensor<2> {
        let e = (t.spell_tokens.clone() * spell.unsqueeze_dim::<3>(2))
            .sum_dim(1)
            .squeeze_dim::<2>(1);
        self.mana_lin
            .forward(Tensor::cat(vec![t.ctx.clone(), e], 1))
    }
}

/// A `[rows, width]` tensor from rows of numbers.
pub fn matrix(rows: Vec<Vec<f32>>, width: usize, device: &Device) -> Tensor<2> {
    let n = rows.len();
    let flat: Vec<f32> = rows.into_iter().flatten().collect();
    Tensor::from_data(TensorData::new(flat, [n, width]), device)
}

/// Log-probabilities over a masked vocabulary.
pub fn log_probs(logits: Tensor<2>, mask: Tensor<2>) -> Tensor<2> {
    log_softmax(masked(logits, mask), 1)
}

/// Where the record of what a checkpoint reads is kept: beside it, as `NAME.shape.ron`.
fn shape_file(checkpoint: &std::path::Path) -> std::path::PathBuf {
    checkpoint.with_extension("shape.ron")
}

/// Write a checkpoint (`path` without the `.bpk` that Burn adds) and, beside it, what the network
/// reads, so a later change to the game's abilities or sizes is caught on loading.
pub fn save_checkpoint(
    net: PolicyNet,
    spec: &crate::obs::ObsSpec,
    path: &std::path::Path,
) -> Result<(), String> {
    use burn::module::Module;
    net.save_file(path).map_err(|e| format!("{e:?}"))?;
    let text = ron::ser::to_string_pretty(&spec.signature(), ron::ser::PrettyConfig::default())
        .map_err(|e| e.to_string())?;
    std::fs::write(shape_file(path), text).map_err(|e| e.to_string())
}

/// Read a checkpoint, or say why it cannot be used with the game as it is now: no record of
/// what it reads (saved before checkpoints had one), or a record that differs.
pub fn load_checkpoint(
    spec: &crate::obs::ObsSpec,
    path: &std::path::Path,
    device: &Device,
) -> Result<PolicyNet, String> {
    use burn::module::Module;
    let shape = shape_file(path);
    let text = std::fs::read_to_string(&shape).map_err(|_| {
        format!(
            "{} has no record of what it reads ({} is missing): it was saved before checkpoints \
             had one, and cannot be told apart from a mismatched one; train again",
            path.display(),
            shape.display()
        )
    })?;
    let saved: crate::obs::Signature =
        ron::from_str(&text).map_err(|e| format!("{}: {e}", shape.display()))?;
    let diff = saved.differences(&spec.signature());
    if !diff.is_empty() {
        return Err(format!(
            "{} was trained on other rules than these: {}; train again",
            path.display(),
            diff.join("; ")
        ));
    }
    Ok(PolicyNet::new(spec.unit_features(), device).load_file(path))
}
