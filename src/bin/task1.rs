fn main() -> color_eyre::Result<()> {
    deadbeefbot::setup(deadbeefbot::remove_twitter_trackers::main)
}