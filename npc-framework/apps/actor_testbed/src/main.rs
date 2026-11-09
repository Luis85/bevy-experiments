fn main() -> bevy::app::AppExit {
    actor_testbed::run_from_args(std::env::args().skip(1).collect())
}
