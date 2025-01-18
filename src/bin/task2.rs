fn main() -> color_eyre::Result<()> {
    // existing AH, can fold in other info.
    deadbeefbot::setup(|| deadbeefbot::articlehistory::main("https://petscan.wmflabs.org/?psid=26656482&format=plain"))
}
