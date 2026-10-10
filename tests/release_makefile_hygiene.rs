// Copyright 2021-2026 ONDEWO GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Release credentials never reach a process command line.
//!
//! `/proc/<pid>/cmdline` is world-readable, so a token on the argv of make, `sh -c`, docker, gh or
//! cargo is visible to every user on the release host for the life of the process. make expands
//! `$(NAME)` and `${NAME}` in a recipe BEFORE it starts `/bin/sh -c '<line>'`, so a recipe only
//! ever reads a credential as `$$NAME`, which the shell expands from the exported environment.

use std::fs;
use std::path::Path;

const MAKEFILE: &str = include_str!("../Makefile");

/// The credentials this repository's release handles.
const SECRETS: [&str; 2] = ["GITHUB_GH_TOKEN", "CARGO_REGISTRY_TOKEN"];

fn recipe_lines(makefile: &str) -> Vec<(usize, &str)> {
    makefile
        .lines()
        .enumerate()
        .filter(|(_, line)| line.starts_with('\t'))
        .map(|(index, line)| (index + 1, line))
        .collect()
}

/// Every `$(SECRET)` / `${SECRET}` make would expand (not `$$(...)`, which the shell expands). The
/// one exempt shape is `$(if $(filter-out <placeholder>,$(SECRET)),<set>,<unset>)`: make evaluates it
/// to `<set>` or `<unset>`, so the value never reaches the recipe line.
fn make_expanded_secrets(line: &str) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    for secret in SECRETS {
        for reference in [format!("$({secret})"), format!("${{{secret}}}")] {
            for (position, _) in line.match_indices(&reference) {
                let before: &str = &line[..position];
                let shell_escaped: bool = before.ends_with('$');
                let set_or_unset: bool =
                    before.ends_with(',') && before.contains("$(if $(filter-out ");
                if !shell_escaped && !set_or_unset {
                    found.push(secret);
                }
            }
        }
    }
    found
}

/// The leaks in a Makefile, as `line: reason`.
fn makefile_leaks(makefile: &str) -> Vec<String> {
    let mut leaks: Vec<String> = Vec::new();
    for (number, line) in recipe_lines(makefile) {
        for secret in make_expanded_secrets(line) {
            leaks.push(format!(
                "{number}: make expands {secret} into the recipe line"
            ));
        }
        if line.contains("$(info)") {
            leaks.push(format!(
                "{number}: $(info) carries make-grepped credentials onto a command line"
            ));
        }
        let starts_make: bool = line.contains("make ") || line.contains("$(MAKE)");
        let reads_credentials: bool =
            line.contains(".env") || SECRETS.iter().any(|secret| line.contains(secret));
        if line.contains("$(shell") && reads_credentials {
            leaks.push(format!(
                "{number}: credentials are read into a make variable with $(shell ...)"
            ));
        }
        for secret in SECRETS {
            if starts_make && line.contains(&format!("{secret}=")) {
                leaks.push(format!("{number}: {secret}=<value> on make's argv"));
            }
        }
    }
    leaks.extend(argv_flag_leaks(makefile));
    leaks
}

/// `docker run -e NAME=<value>` / `--env NAME=<value>` and secret-carrying flags, in any file.
fn argv_flag_leaks(text: &str) -> Vec<String> {
    let mut leaks: Vec<String> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number: usize = index + 1;
        if line.trim_start().starts_with('#') {
            continue;
        }
        for secret in SECRETS {
            for flag in ["-e ", "--env ", "--env="] {
                if line.contains(&format!("{flag}{secret}=")) {
                    leaks.push(format!(
                        "{number}: `{flag}{secret}=<value>` puts the value on docker's argv"
                    ));
                }
            }
        }
        if line.contains("--token") && !line.contains("--with-token") {
            leaks.push(format!("{number}: --token passes a credential on the argv"));
        }
        if line.contains("cargo login ") {
            leaks.push(format!(
                "{number}: `cargo login <token>` passes the token on the argv"
            ));
        }
        if line.contains("Authorization:") && SECRETS.iter().any(|secret| line.contains(secret)) {
            leaks.push(format!(
                "{number}: an Authorization header with a credential on the argv"
            ));
        }
    }
    leaks
}

/// `${{ secrets.X }}` inside a workflow `run:` block is substituted into the script text, i.e. the
/// runner's argv; a secret belongs in `env:` and is read as `$X`.
fn workflow_leaks(workflow: &str) -> Vec<String> {
    let mut leaks: Vec<String> = Vec::new();
    let mut run_indent: Option<usize> = None;
    for (index, line) in workflow.lines().enumerate() {
        let indent: usize = line.len() - line.trim_start().len();
        let content: &str = line.trim_start().trim_start_matches("- ");
        if let Some(block_indent) = run_indent {
            if line.trim().is_empty() || indent > block_indent {
                if line.contains("${{ secrets.") {
                    leaks.push(format!(
                        "{}: ${{{{ secrets.* }}}} inside a run: block",
                        index + 1
                    ));
                }
                continue;
            }
            run_indent = None;
        }
        if content.starts_with("run:") {
            if content.contains("${{ secrets.") {
                leaks.push(format!(
                    "{}: ${{{{ secrets.* }}}} on a run: line",
                    index + 1
                ));
            }
            run_indent = Some(indent);
        }
    }
    leaks.extend(argv_flag_leaks(workflow));
    leaks
}

fn run_release_with_devops_recipe(makefile: &str) -> String {
    makefile
        .split("\nrun_release_with_devops:")
        .nth(1)
        .expect("the Makefile defines run_release_with_devops")
        .split("\n\n")
        .next()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn makefile_recipes_never_put_a_credential_on_a_command_line() {
    assert_eq!(makefile_leaks(MAKEFILE), Vec::<String>::new());
}

#[test]
fn the_devops_release_hands_the_credentials_over_the_environment() {
    let recipe: String = run_release_with_devops_recipe(MAKEFILE);
    assert!(
        recipe.contains("set -a"),
        "the credentials must be exported into the sub-make's environment"
    );
    assert!(
        recipe.contains("'^("),
        "the devops .env files must be read with an anchored grep"
    );
    assert!(!recipe.contains("$(info)"));
}

#[test]
fn dockerfiles_never_carry_a_credential() {
    let root: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));
    for entry in fs::read_dir(root).expect("the crate root is readable") {
        let path = entry.expect("a directory entry").path();
        let name: String = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if name.starts_with("Dockerfile") {
            let text: String = fs::read_to_string(&path).expect("the Dockerfile is readable");
            assert_eq!(argv_flag_leaks(&text), Vec::<String>::new(), "{name}");
            for secret in SECRETS {
                assert!(
                    !text.contains(secret),
                    "{name} names {secret}: a build arg or ENV lands in the image"
                );
            }
        }
    }
}

#[test]
fn workflows_pass_secrets_through_env_only() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows");
    let mut checked: usize = 0;
    for entry in fs::read_dir(&directory).expect("the workflows directory is readable") {
        let path = entry.expect("a directory entry").path();
        let text: String = fs::read_to_string(&path).expect("the workflow is readable");
        assert_eq!(
            workflow_leaks(&text),
            Vec::<String>::new(),
            "{}",
            path.display()
        );
        checked += 1;
    }
    assert!(checked > 0, "no workflow was checked");
}

/// The checks above must catch the shapes this repository used to have.
#[test]
fn the_checks_catch_the_old_leaks() {
    let old_makefile: &str = "\nrun_release_with_devops: ## x\n\
        \t$(eval info:= $(shell cat x/account_github.env | grep GITHUB_GH))\n\
        \t@make release $(info)\n\
        \n\
        login_to_gh:\n\
        \t@echo $(GITHUB_GH_TOKEN) | gh auth login -p ssh --with-token\n\
        \t@docker run --rm -e CARGO_REGISTRY_TOKEN=${CARGO_REGISTRY_TOKEN} image\n\
        \t@make publish_crate CARGO_REGISTRY_TOKEN=x\n\
        \tcargo publish --token x\n";
    let leaks: Vec<String> = makefile_leaks(old_makefile);
    for expected in [
        "3: credentials are read",
        "4: $(info)",
        "7: make expands GITHUB_GH_TOKEN",
        "8: make expands CARGO_REGISTRY_TOKEN",
        "8: `-e CARGO_REGISTRY_TOKEN=",
        "9: CARGO_REGISTRY_TOKEN=<value> on make's argv",
        "10: --token",
    ] {
        assert!(
            leaks.iter().any(|leak| leak.starts_with(expected)),
            "{expected} not caught in {leaks:?}"
        );
    }
    assert!(!run_release_with_devops_recipe(old_makefile).contains("set -a"));

    let old_workflow: &str = "    steps:\n      - name: publish\n        run: |\n          cargo publish\n          echo ${{ secrets.CARGO_REGISTRY_TOKEN }} | x\n      - run: x ${{ secrets.A }}\n";
    assert_eq!(workflow_leaks(old_workflow).len(), 2);

    let fine: &str = "\t@echo \"$(if $(filter-out P,$(GITHUB_GH_TOKEN)),<set>,<unset>)\"\n\
        \t@echo \"$$GITHUB_GH_TOKEN\" | gh auth login -p ssh --with-token\n\
        \t@docker run --rm -e CARGO_REGISTRY_TOKEN image\n";
    assert_eq!(makefile_leaks(fine), Vec::<String>::new());
    let fine_workflow: &str =
        "      - name: publish\n        env:\n          T: ${{ secrets.T }}\n        run: make publish_crate\n";
    assert_eq!(workflow_leaks(fine_workflow), Vec::<String>::new());
}
