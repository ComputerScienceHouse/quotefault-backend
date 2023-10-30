use vergen::EmitBuilder;

fn main() {
    EmitBuilder::builder()
        .build_timestamp() // outputs 'VERGEN_BUILD_TIMESTAMP'
        .git_sha(false) // outputs 'VERGEN_GIT_SHA', and sets the 'short' flag false
        .git_commit_timestamp() // outputs 'VERGEN_GIT_COMMIT_TIMESTAMP'
        .emit()
        .expect("Unable to generate the cargo keys!");
}
