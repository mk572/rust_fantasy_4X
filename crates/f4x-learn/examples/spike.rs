//! The framework spike of task 169: can Burn train a few-million parameter network on this Mac,
//! and how fast, on the CPU and on the GPU?
//!
//! `scripts/cool_run.sh cargo run --release -p f4x-learn --features spike --example spike -- flex|metal [steps] [batch]`

use std::time::Instant;

use burn::nn::{Linear, LinearConfig, Relu};
use burn::optim::{AdamConfig, GradientsParams};
use burn::prelude::*;
use burn::tensor::Distribution;

#[derive(Module, Debug)]
struct Net {
    l1: Linear,
    l2: Linear,
    l3: Linear,
    out: Linear,
    act: Relu,
}

impl Net {
    fn new(input: usize, device: &Device) -> Self {
        Self {
            l1: LinearConfig::new(input, 1024).init(device),
            l2: LinearConfig::new(1024, 1024).init(device),
            l3: LinearConfig::new(1024, 512).init(device),
            out: LinearConfig::new(512, 64).init(device),
            act: Relu::new(),
        }
    }

    fn forward(&self, x: Tensor<2>) -> Tensor<2> {
        let x = self.act.forward(self.l1.forward(x));
        let x = self.act.forward(self.l2.forward(x));
        let x = self.act.forward(self.l3.forward(x));
        self.out.forward(x)
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let which = args.next().unwrap_or_else(|| "flex".into());
    let steps: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(30);
    let batch: usize = args.next().and_then(|s| s.parse().ok()).unwrap_or(256);
    let base = match which.as_str() {
        "metal" => Device::metal(Default::default()),
        _ => Device::flex(),
    };
    let device = base.autodiff();
    let input = 2048;
    let mut net = Net::new(input, &device);
    println!(
        "{which}: {} parameters, batch {batch}, {steps} steps",
        net.num_params()
    );
    let mut opt = AdamConfig::new().init();
    let x = Tensor::<2>::random([batch, input], Distribution::Default, &device);
    let target = Tensor::<2>::random([batch, 64], Distribution::Default, &device);
    let mut first = 0.0;
    let mut last = 0.0;
    let t0 = Instant::now();
    for i in 0..steps {
        let out = net.forward(x.clone());
        let loss = (out - target.clone()).powf_scalar(2.0).mean();
        let value: f32 = loss.clone().into_scalar();
        if i == 0 {
            first = value;
        }
        last = value;
        let grads = GradientsParams::from_grads(loss.backward(), &net);
        net = opt.step(1e-3, net, grads);
        if i == 2 {
            println!(
                "(three warm-up steps took {:.2}s)",
                t0.elapsed().as_secs_f32()
            );
        }
    }
    let total = t0.elapsed().as_secs_f32();
    println!(
        "loss {first:.4} -> {last:.4}; {:.1} ms a step overall",
        1000.0 * total / steps as f32
    );
}
