use hana_rigging::DeviceIdentity;

fn classify(identity: DeviceIdentity) -> &'static str {
    match identity {
        DeviceIdentity::Proven => "proven",
        DeviceIdentity::RestoreOnly => "restore only",
        DeviceIdentity::Authored => "authored",
        DeviceIdentity::Displaced { .. } => "displaced",
        DeviceIdentity::WrongUnit { .. } => "wrong unit",
        DeviceIdentity::Unverified(_) => "unverified",
    }
}

fn main() {
    let _ = classify(DeviceIdentity::Proven);
}
