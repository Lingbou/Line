use std::process::Command;

#[test]
fn same_binary_askpass_preserves_special_characters_without_starting_the_tui() {
    let token = "123e4567-e89b-12d3-a456-426614174000";
    let variable = format!("LINE_INTERNAL_ASKPASS_PASSWORD_{token}");
    let password = "space $dollar 'quotes' 中文";

    let output = Command::new(env!("CARGO_BIN_EXE_line"))
        .arg("Password for test:")
        .env("LINE_INTERNAL_ASKPASS", token)
        .env(variable, password)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(output.stdout, format!("{password}\n").as_bytes());
    assert!(output.stderr.is_empty());
}
