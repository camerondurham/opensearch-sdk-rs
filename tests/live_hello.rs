use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "requires a local OpenSearch 3.x checkout and JDK"]
fn live_hello_harness_succeeds() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = crate_dir.join("scripts/live_hello.sh");
    let opensearch_dir = std::env::var_os("OPENSEARCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate_dir.join("../OpenSearch"));

    assert!(
        script.is_file(),
        "live harness script not found at {}",
        script.display()
    );
    assert!(
        opensearch_dir.is_dir(),
        "OpenSearch checkout not found at {}. Set OPENSEARCH_DIR to override.",
        opensearch_dir.display()
    );

    let status = Command::new(&script)
        .current_dir(&crate_dir)
        .env("OPENSEARCH_DIR", &opensearch_dir)
        .status()
        .expect("failed to execute scripts/live_hello.sh");

    assert!(status.success(), "live hello harness exited with {status}");
}
