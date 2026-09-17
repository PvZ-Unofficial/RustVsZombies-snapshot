fn main() {
    rsvz::at_frame(1, 401, |frame| {
        for zombie in frame.zombies() {
            zombie.kill();
        }
    });
}
