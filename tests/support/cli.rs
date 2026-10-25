use assert_cmd::Command;

use super::fake_state::FakeState;
use super::repo::TempRepo;

pub fn contrib_command(repo: &TempRepo, state: &FakeState) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("contrib"));
    command
        .current_dir(repo.path())
        .env("CONTRIB_STATE_URL", &state.url);
    command
}
