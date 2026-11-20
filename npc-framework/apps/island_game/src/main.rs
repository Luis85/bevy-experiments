fn main() -> bevy::app::AppExit {
    island_game::run_from_args(std::env::args().skip(1).collect())
}
