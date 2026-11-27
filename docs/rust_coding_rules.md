# Rust Coding Rules

General rules for Rust code and its tests. They hold for any project and name none of its crates,
files, functions, scripts, commands or tasks. What is particular to a project (where its data is,
which helpers its tests use, how to run them) belongs in that project's own docs.

## Safe coding

Unsafe code is only allowed in the test environment. Production code avoids it wherever feasible,
and unsafe code found there is replaced.

Implement error handling according to good Rust coding practices.
Avoid unwrap. Avoid panics. (Tests have their own rules, see "Error handling in tests" below.)

Do not drop a result with `let _ =` or `.ok();`, and do not answer a failure with a `bool` or
`Option` where several things can go wrong: return an error that says which, and let the caller
choose what to do with it (try the next, give up the plan, tell the user, log it). A caller that
wants only yes or no says so with `.is_ok()` and a comment. A failure that may be ignored (removing
something that need not be there) gets a helper whose doc comment says which failures it ignores and
which it does not.

Use Clippy's `unwrap_used` and `expect_used` lints to keep unwraps out of production code; tests may
allow them with `#![cfg_attr(test, allow(clippy::unwrap_used))]`.

## Lists, tables and fixed-size arrays

No array type with a written length (`[T; 48]`) for a list or a table: the count must be edited
whenever an entry is added, and it says nothing about whether the right entries are there.

- A table of constants is a slice: `const NAMES: &[(&str, usize)] = &[...]`.
- One entry per variant of an enum is an exhaustive `match` with no wildcard arm, so a new variant
  does not compile until it has its entry. The same goes for moving from a variant to its place in a
  list, and from a name or a number back to a variant.
- To go over every variant, derive `strum::EnumIter` and use `iter()` (or a small `all()` on the
  enum, so the callers need no import). A test that must see all of them uses it too, so nothing
  can be forgotten.
- An array stays only where the length is the point, and a comment says why: a coordinate or a
  colour (`[f32; 2]`, `[u8; 4]`), the 8 neighbours of a grid cell, a file format's fixed buffer, an
  input of a fixed shape.

## Resource efficiency

Avoid cloning, especially of large values.

## Layers of calls

A function gets its information at the source. A function between a caller and the source needs a
purpose, which the doc comment states when it is not plain: it **wraps private data** (the caller may
not see the field), **converts** (a type, a unit), **binds** a default, a constant or the caller's own
identity, **caches**, or **enforces a rule** (a boundary that limits what the caller can read or do).
A function that only passes its arguments on, renames the call, drops a result, or exists because it
once did something else is removed and its callers go to the source.

## Dependencies

Modules and crates depend on each other in one direction. A lower layer never calls a higher one,
and does not depend on it for its tests either: a test that needs the higher layer lives there.
Rules and data sit below the code that applies them. Decisions of users and automated players are
not made by the code that holds the state: it validates and applies what they ask for.

## Comments

Comments are brief and sit where the code cannot say it itself: why something is done, the external
rule or source it follows, a trap. Documentation lives in the docs, not in the code.

- No task numbers and no history ("added for ...", "first version", "will be replaced"). The task
  list and git say what was done and when; a comment that names a task goes stale when it is done.
- No general project information (what the program, a crate or a component is for, how it works as a
  whole). A module doc is one or two lines on what the file holds, and a pointer to the doc if one
  exists.
- No comment that restates the next line, and no commented-out code.
- A comment that no longer matches the code is a bug: fix or delete it in the change that finds it.
- Test comments follow the same rule: name the situation a test builds only when the test name and the
  assertions do not already say it.

# Testing

Implement several test layers.

## Functional testing

### This is a game, not a banking system

A banking system has business rules the application must never stray from, and its tests pin them.
A game changes its gameplay as it evolves, and that is allowed and expected. So a test does **not
define the behaviour** that data and settings decide. The rules and their switches live in the data
and the settings; a test reads them and checks that the program implements what they say. When the
design changes, the data or the setting changes, and the tests, which only follow it, stay green. A
test that has to be rewritten whenever a value is tuned is carrying a rule it should not carry.

A good test performs an **action** and checks the **outcome the user sees**. Good: do something the
rules forbid for a certain object; it is refused and the user gets a message that says why. Bad:
check that a certain object has the property that forbids it (that is a fact of the data, and
changing the data must not break a test). Where the outcome depends on a setting, run the action
under each value of the setting and check the outcome the setting promises.

### Every file has tests

Every `.rs` file with logic has tests (a `#[cfg(test)] mod tests` in it, or a `tests` module beside
it for a big file). A file may go without only when someone has checked it, decided that tests add
nothing, and written that down: a line in the file's module doc, `//! Tests: none, because <reason>.`
Typical reasons: the file only declares modules or re-exports (`mod.rs`, `lib.rs`), only holds data
types without behaviour, or only draws and needs a window (then the deciding part is moved into a
testable function, see "UI test"). Code that is covered only by another file's tests is named there:
`//! Tests: in <file>`. A file with neither tests nor such a line is a gap, to be fixed in the
change that finds it.

### Test on the actual data and rules

**Always test on the shipped data and the real rules, never on fixtures.** A test loads the data the
program ships with, through the same loader and checks as the program, and runs the real engine, so
that it behaves as close to actual use as possible. Hand-built data, a second copy of a rule in the
test environment, or an empty default lets the test drift from the program and pass for the wrong
reason.

- **A test never builds its own rule set.** Do not copy a registry of definitions, edit one and put
  it back, or assign over a part of the loaded rules: pick the entry or setting of the shipped data
  that has the property the test needs, or fix the shipped data if its rules are wrong. The
  exceptions are a test of the data validator (it may break a copy on purpose) and a test of a
  function that takes the rule set as its input and needs a case the shipped data lacks; such a test
  says why in a comment. Where a result depends on chance, repeat the action until it happens or give
  the actor the means, rather than raising a number in the rules. Setting a user setting or a
  configuration value is not building a rule set.
- **A test never carries a limit or a number of the rules of its own** (a capacity, a count, a
  price): read it from the running program or the shipped data, never a literal and never a
  setting's upper bound, which is not the value in use.
- Build only situations the program can reach. A situation it cannot create belongs in a test only to
  check error handling (a rejected command, a refused file).
- Refer to entries of the data by their stable identifier, as the saved files do, written where it is
  used. Do not copy an identifier from another test, do not invent values, and do not pick an entry by
  its position in a table.
- Change the rules in a test only when the change is the point of the test. A change that only makes
  the sums easier belongs to the test that needs it, with the reason, not to a shared setup.
- A test whose subject needs data must fail when the data is missing. An empty registry that makes
  the test skip its assertions is a bug.

### Test the behaviour against the setting, not a copy of it

**Data files are guarded against unintended change as files** (a recorded checksum per file and a
test that holds the files against it), **not by tests that repeat their values.** A change made on
purpose is recorded, and the record goes in the same commit.

**A test never writes down what a setting says.** An expected value typed into a test (a cost, a
limit, a list of what is allowed, the number of things an entry gives) is a *shadow setting*: a
second copy of the data that has to be edited together with it, that fails when the data is changed
on purpose, and that shows nothing about the program. The test reads the setting and checks that
**the program does what it says**.

- **The expectation comes from the rules the program was built from.** Read the value from the
  shipped data in the test, drive the real program, and compare the outcome with what was read.
- **Example.** A test finds an entry that forbids an action for some kind of object, tries the
  action and checks that it is not offered and is refused; it finds one that allows it, and checks
  that it is offered and succeeds.
- **A test names the setting it reads** (a comment or the helper's name), so that a reader sees what
  the behaviour is checked against.
- **Fixed values as input are no shadow settings.** Which entry to create, a position, a seed, the
  number of steps to run are the scenario. What counts is a number or a name *asserted* as the
  outcome that the data alone decides.
- **Pick the case from the data, not from memory.** Search the data for an entry where the property
  holds, and fail the test when there is none.
- **A test that can only restate a field**, because no code reads it, is deleted, and the field is
  reported as dead.
- **No unproven assumption decides a test.** A test must not rely on a claim nobody has shown, such
  as which of two sides wins a contest. When it needs a stronger or a weaker side, it builds it from
  the program's own measure, with a wide margin, so that it tests the decision that follows from the
  measure, not the measure.
- **Check yourself:** change a value of the data on purpose. A test may fail only when the behaviour
  then differs from the setting. If it fails because the number it wrote down no longer matches, it
  is a shadow setting.

### Where a test lives

Prefer the public interface.

- **An integration test** (in the crate's `tests/` folder) when the situation can be built and the
  result read through the public API. This is the first choice for a rule that crosses modules, and
  for new tests. When a way to set up a situation is missing, add a narrow setup function; do not
  make a field public.
- **A unit test in `src/`** when the test needs a module's internals (a private function, a field) or
  no larger setup at all: a formula, a parser, an ordering, a state machine of one type. Keep it to
  the minimum the mechanic needs.
- A test whose subject is the data itself (counts, cross-references) lives with the code that loads
  it.

### Error handling in tests

A test fails when it panics or when it returns `Err`; both are fine. Choose by what the test does:

- **Code under test returns `Result`** (loading, parsing, saving, a command that can be rejected):
  make the test `-> Result<(), Box<dyn std::error::Error>>` and use `?`. The failure then shows the
  real error instead of `called Result::unwrap() on an Err value`.
- **Setup that must succeed** (looking up an entry, an object that was just added): use
  `.expect("why this cannot be missing")`, never a bare `.unwrap()`. The message documents the
  precondition. Where the test returns `Result`, `.ok_or("what is missing")?` does the same.
- **Assertions stay `assert!`, `assert_eq!`, `assert_ne!`, `matches!`.** Do not turn them into `Err`
  returns: that loses the left/right diff and the line number.
- **No `panic!` as a check.** Use an assertion with a message. `#[should_panic(expected = "...")]` is
  only for code that is meant to panic; a returned error is asserted on as a value.
- **Never skip silently.** `if let Some(x) = ... { assert!(..) }` passes when the value is missing and
  checks nothing. Use `let Some(x) = ... else { return Err(..) }` or `expect`.
- **Test the error paths.** Assert the specific error variant (`matches!`, `assert_eq!`), not just
  `is_err()`, which can pass for the wrong reason.

### Quality of tests

- **Behaviour, not implementation.** Assert what a user or a caller can observe (events, state), so a
  refactor does not break the test.
- **Deterministic.** Seed every random generator; no wall-clock time.
- **Isolated files.** A test that writes files uses a folder unique per process and per call, removed
  afterwards, never a fixed name in the temp folder: two test runs at the same time must not share it.
- **Fast.** A unit test runs in milliseconds. Build the setup once per test (not once per loop
  iteration) and borrow rather than clone large values. A slow test is marked `#[ignore]` and run on
  request.
- **Every test asserts something that can fail.** Mutation testing (`cargo mutants`) finds tests that
  pass without protecting anything.
- **Invariants and round trips** (save then load, a value that must never go negative) suit
  property-based tests (`proptest`); large structured output (event logs, saved files) suits snapshot
  tests (`insta`). Add such a dependency with the first test that needs it.
- **Gates before a commit:** the formatter, the linter on all targets including tests, and the tests.

## Regression testing

Regression testing is done on the program as a whole, not on individual crates, and without the user
interface. Its goal is to catch unintended divergences in specific behaviour.

A regression test loads a prepared saved state, performs a number of defined actions, and compares
the resulting state against a stored expected result. A failing test shows both. The expected results
are changed only on purpose, by a documented command, and the difference is read before it is
committed.

## UI test

The goal of this test is to catch divergence in the user interface.

Graphical tests load a prepared state, perform actions given by a script, take a screenshot and
compare it to a target screenshot.

Click-flow tests (no window, no screenshot) are unit tests of the user interface code. They need
seams: a way to build the application for a test without assets, input frames built by hand, and a
fixed window size. Anything that draws needs a window, so put the part that decides (what a key
answers, whether a notice closes, what an answer changes) in a function that takes the input or the
answer and returns the new state, draw after it, and test that function with each possible answer.

## Soak testing

The goal of soak testing is to catch divergence in overall behaviour over a long run.

A soak test starts from a saved state or from a defined seed and settings, and runs the program
automatically, without interference. It takes snapshots at set intervals and compares them with a
target or with an outcome defined for that test.
