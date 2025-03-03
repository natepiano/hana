//! for testing - an app that will never accept a connection
fn main() {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
