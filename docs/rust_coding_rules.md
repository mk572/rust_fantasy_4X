# Rust Coding Rules

## Safe coding

Unsafe code is only allowed in the test environment.

Production code (the game) should avoid unsafe code wherever feasible.
Current unsafe code should be replaced.

Implement error handling according to good Rust coding practices.
Avoid unwrap. Avoid panics.

## Resource efficiency

Avoid cloning, especially on large entities.

# Basic Structure

The game is built on four major components: rules engine, game engine, client and ai.

## Rules engine

The rules engine is static and implements the rules of the game.
It reads game rules and content and provides it to the game engine.
For example, unit stats and abilities, buildings, spells, terrain etc. are all provided via the rules engine.
The rules engine sits in f4x-data.

## Game engine

The game engine is dynamic. It manages the flow of the game.
Starting a game, setting up a wizard, turn order, turn flow, combat, build orders, 
spellcasting, save and load etc. are all handled by the game engine.
The game engine sits in f4x-core.

## Client

The client handles the user interface.
It creates screens and menus, displays graphical assets, plays sounds, takes user input etc.
The client sits in f4x-client.

## AI

The AI runs the non-human players. It interacts directly with the game engine.
The AI follows the same rules as the human player, unless differences are defined 
in the rules engine (for example through difficulty settings).

The game engine or rules engine never hard-code AI decisions. (Unlike the original game)
For example, diplomacy decisions such as declaring war or accepting treaties are always 
made by the AI, not the rules or the game.

# Testing

Implement several test layers:

## Functional testing

Small tests within rs files themselves.

## Regression testing

Regression testing should be implemented on the overall game, not individual crates.

The goal of regression testing is to catch unintended divergences in specific gameplay.

Regression tests load a prepared test save and perform a number of defined game actions.
Then they save the game at a defined step. The save is then compared against either a 
predefined outcome save or a diff against the loaded save.

Regression testing is done without the client.

## UI test

The goal of this test is to catch divergence in the user interface.

Graphical tests load a predefined save, perform a defined set of game actions determined 
by a script, then take a screenshot and compare it to a target screenshot.

UI testing requires the client.

Click-flow tests (no window, no screenshot) are unit tests in the client crate. The seams:
`App::for_test()` (`test_app.rs`: the shipped content and a new game, no art), `Input::key(..)` and
`Input::click(..)` build one frame by hand, `frame::window_size()` gives tests a fixed window, and
`App::modals` shows which prompts are open. Anything that draws needs a window, so put the part that
decides (what a key answers, whether a notice closes, which events open a prompt) in a function
that takes `&Input` and returns the new state (`resign_answer`, `Input::dismisses`), draw after it,
and test that function.

## Soak testing

The goal of soak testing is to catch divergence in overall gameplay.

Soak testing either loads a save or starts a new game with defined seed and settings.
It then runs an automated game with AI players, without interference or special instructions.
The test takes a save snapshot at predefined turn intervals (usually every 50 or 100 turns),
and compares it to a target save or a defined outcome specific to that test.
(number of spells cast, number of battles fought etc.)

## AI Improvement testing

These tests are similar to soak tests, except they are testing the outcome of AI changes.

An AI improvement test first runs a baseline game with specific settings and seed, 
using only AI players with the pre-change AI.

In the second step, the test runs the same game again, swapping one or several (but not all)
of the AI players for the updated AI.
The outcomes are then compared according to various success metrics, 
such as winning the game, number of cities funded, number of cities conquered,
explored terrain, conquered or controlled encounter zones and nodes, spells researched,
spells cast etc.

