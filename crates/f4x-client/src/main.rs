fn main() {
    let mut game = f4x_core::Game::new(0);
    println!("{:?}", game.apply(f4x_core::Command::EndTurn));
}
