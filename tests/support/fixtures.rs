pub const REGISTRATION_PATH: &str = "requests/jpx/20261008T120000Z-register-jpx.yaml";
pub const REGISTRATION: &str = "candidate: jpx\nregister:\n  name: JPX\n  description: JPX is a tool.\n  website: https://jpx.dev/\n";

pub const GOOD_ENTRIES: [(&str, u32); 3] = [
    ("jpx-1.0.0/", 0o755),
    ("jpx-1.0.0/bin/", 0o755),
    ("jpx-1.0.0/bin/jpx", 0o755),
];

pub const GOOD_ENTRIES_1_2_0: [(&str, u32); 3] = [
    ("jpx-1.2.0/", 0o755),
    ("jpx-1.2.0/bin/", 0o755),
    ("jpx-1.2.0/bin/jpx", 0o755),
];
