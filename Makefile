export
# =====================================================================================
# ondewo-t2s-client-rust - Makefile
#
# The ONDEWO T2S (Text-to-Speech) gRPC client for rust. The crate is generated
# from the protos of the ondewo-t2s-api submodule by the ondewo-rust-proto-compiler image that
# the ondewo-proto-compiler submodule builds - only src/api is generated, everything else in
# this repository is hand-written.
#
# Quick start:
#   make help                 # list every documented target
#   make makefile_chapters    # list the section headers below
#   make build                # submodules -> compiler image -> stubs -> cargo build
#   make test                 # cargo test
# =====================================================================================

# ---------------- BEFORE RELEASE ----------------
# 1 - Update the version number (ONDEWO_T2S_VERSION below)
# 2 - Update RELEASE.md
# 3 - make build
# -------------- Release Process Steps --------------
# 1 - Get Credentials from devops-accounts repo
# 2 - Create Release Branch and push
# 3 - Create Release Tag and push
# 4 - GitHub Release
# 5 - crates.io Release - done by .github/workflows/release.yml, which the tag push in step 3
#     triggers. `make ondewo_publish_crate` is the manual fallback.

########################################################
# 		Variables
########################################################

# MUST BE THE SAME AS THE T2S API IN MAJOR AND MINOR VERSION NUMBER
# example: API 2.9.0 --> Client 2.9.X
ONDEWO_T2S_VERSION=6.6.0

# Submodule pins - `make checkout_defined_submodule_versions` checks out exactly these.
# Pin the API to `tags/<api version>` before cutting a release; a branch is for development only.
ONDEWO_T2S_API_GIT_BRANCH=tags/6.6.0
ONDEWO_PROTO_COMPILER_GIT_BRANCH=tags/5.15.1

# Submodule directories - these MUST match the paths in .gitmodules
ONDEWO_T2S_API_DIR=ondewo-t2s-api
ONDEWO_PROTO_COMPILER_DIR=ondewo-proto-compiler

# The generation contract with the proto compiler. The image TAG is the only contract between
# this repository and the compiler - `make build_compiler` rebuilds it from the submodule.
# The image ENTRYPOINT takes two positional arguments, BOTH relative to /input-volume:
#   <relative_protos_dir>   the protoc -I root every `import "x/y.proto";` resolves against
#   <target_subdir>         the sub-directory whose protos are the compilation entry points;
#                           google/ is pulled in only as a resolved dependency
PROTO_COMPILER_IMAGE=ondewo-rust-proto-compiler:latest
ONDEWO_PROTOS_DIR=${ONDEWO_T2S_API_DIR}
ONDEWO_PROTOS_TARGET_DIR=ondewo

# Staging directory that is handed to the image as /input-volume (see generate_ondewo_protos)
PROTO_INPUT_DIR=.proto-input

# Coverage gate - KEEP IN SYNC with .github/workflows/ci.yml, which runs the same two values.
# Excluded from the metric: the generated stubs (machine output, not authored logic) and the
# tests/examples themselves (the measuring instrument).
COVERAGE_EXCLUDE_REGEX=(^|/)(src/api/|tests/|examples/)
COVERAGE_MIN_LINES=100

# You need to setup an access token at https://github.com/settings/tokens - permissions are important
GITHUB_GH_TOKEN?=ENTER_YOUR_TOKEN_HERE
# You need to setup an API token at https://crates.io/settings/tokens, scoped to publish-new +
# publish-update. `cargo publish` reads it from the environment (this Makefile `export`s every
# variable), so it never appears on a command line and never reaches the build log.
CARGO_REGISTRY_TOKEN?=ENTER_HERE_YOUR_CARGO_REGISTRY_TOKEN

# crates.io refuses an upload whose .crate tarball is larger than this (10 MiB, the default limit).
# The check is server-side, so `cargo publish --dry-run` cannot catch it - publish_crate_dry_run does.
CRATE_MAX_BYTES=10485760

# Terminate on the ***** separator that delimits release entries, NOT on /\*\*/ - that matches the
# first markdown **bold** span inside the entry and silently truncates the notes there, with no
# error from `gh release create`.
CURRENT_RELEASE_NOTES=`cat RELEASE.md \
	| perl -ne 'print if /Release ONDEWO T2S Rust Client ${ONDEWO_T2S_VERSION}/../^\*{5}/'`

GH_REPO="https://github.com/ondewo/ondewo-t2s-client-rust"
DEVOPS_ACCOUNT_GIT="ondewo-devops-accounts"
DEVOPS_ACCOUNT_DIR="./${DEVOPS_ACCOUNT_GIT}"

# Define colors globally (reused for [INFO]/[SUCCESS]/[WARN]/[ERROR] log lines in recipes)
BLUE   := \033[1;34m
GREEN  := \033[0;32m
YELLOW := \033[1;33m
RED    := \033[0;31m
NC     := \033[0m

.DEFAULT_GOAL := help

########################################################
#       ONDEWO Standard Make Targets
########################################################

setup_developer_environment_locally: install_rust_toolchain install_precommit_hooks ## Ready a fresh laptop: rust toolchain + pre-commit hooks

install_rust_toolchain: ## Install rustup/cargo if missing and add the rustfmt and clippy components
	@command -v cargo >/dev/null 2>&1 || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
# rustup's installer puts cargo on PATH for NEW shells only, so source its env for this one -
# otherwise the very first run of this target installs rustup and then cannot call it.
	[ -f "$$HOME/.cargo/env" ] && . "$$HOME/.cargo/env"; rustup component add rustfmt clippy

install_precommit_hooks: ## Installs pre-commit hooks and sets them up for the ondewo-t2s-client-rust repo
	@command -v pre-commit >/dev/null 2>&1 || uv tool install pre-commit || pipx install pre-commit || pip install --user pre-commit
	pre-commit install
	pre-commit install --hook-type commit-msg

precommit_hooks_run_all_files: ## Runs all pre-commit hooks on all files and not just the changed ones
	pre-commit run --all-files

help: ## Print usage info about help targets
	# (first comment after target starting with double hashes ##)
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' Makefile | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-40s\033[0m %s\n", $$1, $$2}'

makefile_chapters: ## Shows all sections of Makefile
	@echo `cat Makefile| grep "########################################################" -A 1 | grep -v "########################################################"`

TEST: ## Prints some important variables
	@echo "Release Notes: \n \n$(CURRENT_RELEASE_NOTES)"
	@echo "GH Token: \t $(if $(filter-out ENTER_YOUR_TOKEN_HERE,$(GITHUB_GH_TOKEN)),<set>,<unset>)"
	@echo "Cargo Token: \t $(if $(filter-out ENTER_HERE_YOUR_CARGO_REGISTRY_TOKEN,$(CARGO_REGISTRY_TOKEN)),<set>,<unset>)"
	@echo "Compiler Image:  $(PROTO_COMPILER_IMAGE)"
	@echo "Protos: \t $(ONDEWO_PROTOS_DIR)/$(ONDEWO_PROTOS_TARGET_DIR)"

check_build: ## Checks if all built proto-code is there
# prost writes ONE file per proto PACKAGE (flat_output_dir), named "<package>.rs" - so the
# check is per package, not per .proto file, and src/api/mod.rs is the barrel that includes them.
	@rm -f build_check.txt
	@find ${ONDEWO_PROTOS_DIR}/${ONDEWO_PROTOS_TARGET_DIR} -type f -name "*.proto" -exec sed -n 's|^[[:space:]]*package[[:space:]][[:space:]]*\([A-Za-z0-9_.]*\)[[:space:]]*;.*|\1|p' {} \; > build_check.txt
	@sort -u build_check.txt -o build_check.txt
	@test -s build_check.txt || { echo "$(RED)[ERROR]$(NC) No proto packages found under ${ONDEWO_PROTOS_DIR}/${ONDEWO_PROTOS_TARGET_DIR}"; rm -f build_check.txt; exit 1; }
	@test -f src/api/mod.rs || { echo "$(RED)[ERROR]$(NC) src/api/mod.rs is missing - run 'make generate_ondewo_protos'"; rm -f build_check.txt; exit 1; }
	@while read -r package; do \
		test -f "src/api/$$package.rs" || { echo "$(RED)[ERROR]$(NC) No rust stub for proto package $$package (expected src/api/$$package.rs)"; rm -f build_check.txt; exit 1; }; \
	done < build_check.txt
	@rm -f build_check.txt
	@echo "$(GREEN)[SUCCESS]$(NC) A generated stub exists for every proto package"

########################################################
#       Repo Specific Make Targets
########################################################
#		Build

build: update_submodules checkout_defined_submodule_versions build_compiler update_cargo_version generate_ondewo_protos check_build cargo_build ## Build source code: submodules -> compiler image -> stubs -> cargo build
	@echo "$(GREEN)[SUCCESS]$(NC) Build of ondewo-t2s-client-rust ${ONDEWO_T2S_VERSION} finished"

build_compiler: ## Build proto compiler docker image from the ondewo-proto-compiler submodule
	@echo "$(BLUE)[INFO]$(NC) Building ${PROTO_COMPILER_IMAGE} from ${ONDEWO_PROTO_COMPILER_DIR}/rust ..."
	cd ${ONDEWO_PROTO_COMPILER_DIR}/rust && sh build.sh
	@echo "$(GREEN)[SUCCESS]$(NC) Built ${PROTO_COMPILER_IMAGE}"

generate_ondewo_protos: ## Generate rust code from proto files into src/api
	@echo "$(BLUE)[INFO]$(NC) Generating rust stubs from ${ONDEWO_PROTOS_DIR}/${ONDEWO_PROTOS_TARGET_DIR} ..."
	@test -d ${ONDEWO_PROTOS_DIR}/${ONDEWO_PROTOS_TARGET_DIR} || { echo "$(RED)[ERROR]$(NC) '${ONDEWO_PROTOS_DIR}/${ONDEWO_PROTOS_TARGET_DIR}' is missing - run 'make update_submodules' first"; exit 1; }
# The image COPIES the whole mounted input volume into itself before it compiles, so the repo
# root is deliberately not the input volume: it carries target/ (gigabytes after a release
# build) and .git. Stage exactly what the image consumes instead - the protos, the crate
# manifest and the hand-written sources. src/api is left out on purpose: it is entirely
# generated, and the image wipes it in the output volume before copying the new stubs back.
	rm -rf ${PROTO_INPUT_DIR}
	mkdir -p ${PROTO_INPUT_DIR}/src
	cp Cargo.toml ${PROTO_INPUT_DIR}/Cargo.toml
	find src -mindepth 1 -maxdepth 1 ! -name api -exec cp -R {} ${PROTO_INPUT_DIR}/src/ \;
	cp -R ${ONDEWO_PROTOS_DIR} ${PROTO_INPUT_DIR}/${ONDEWO_PROTOS_DIR}
# Same image tag and the same positional arguments as the compiler's own
# ondewo-proto-compiler/rust/example/run-compile.sh. Two deliberate differences:
#  * NO -it - a TTY-enabled container breaks every non-interactive caller with
#    "cannot attach stdin to a TTY-enabled container because stdin is not a terminal".
#  * --user, as in ondewo-proto-compiler/rust/Makefile, so the generated src/api,
#    Cargo.toml, Cargo.lock and crate-dist/ are owned by the caller and not by root.
#    The image chmods its working directories a+w so this works.
# The output volume is the repository root, because the crate IS this repository.
	docker run --rm \
		--user ${shell id -u}:${shell id -g} \
		-v ${shell pwd}/${PROTO_INPUT_DIR}:/input-volume \
		-v ${shell pwd}:/output-volume \
		${PROTO_COMPILER_IMAGE} ${ONDEWO_PROTOS_DIR} ${ONDEWO_PROTOS_TARGET_DIR}
	rm -rf ${PROTO_INPUT_DIR}
	@echo "$(GREEN)[SUCCESS]$(NC) Generated the rust stubs in src/api"

update_cargo_version: ## Update Version in Cargo.toml
	@perl -0pi -e 's/(\[package\][^\[]*?\nversion = ")[^"]*(")/$${1}${ONDEWO_T2S_VERSION}$${2}/s' Cargo.toml
	@echo "$(GREEN)[SUCCESS]$(NC) Cargo.toml version set to ${ONDEWO_T2S_VERSION}"

cargo_build: ## Compile the crate in release mode
	cargo build --release

test: ## Run the test suite (--all-targets also compiles examples/)
	cargo test --all-targets

coverage: ## Line coverage of the HAND-WRITTEN sources, gated at COVERAGE_MIN_LINES (as in CI)
# src/api is machine output and tests/ + examples/ are the measuring instrument, so neither
# belongs in the metric - what remains is exactly the hand-written library surface. The generated
# stubs are still exercised, by the behavioural tests under tests/.
	@command -v cargo-llvm-cov >/dev/null 2>&1 || { echo "$(RED)[ERROR]$(NC) cargo-llvm-cov is missing - install it with 'cargo install cargo-llvm-cov --locked'"; exit 1; }
	cargo llvm-cov --summary-only \
		--ignore-filename-regex '${COVERAGE_EXCLUDE_REGEX}' \
		--fail-under-lines ${COVERAGE_MIN_LINES}

cargo_fmt: ## Format the hand-written sources (src/api is generated and never hand-formatted)
	find src -name "*.rs" -not -path "src/api/*" -exec rustfmt --edition 2021 {} +

cargo_fmt_check: ## Check the formatting of the hand-written sources without changing them
	find src -name "*.rs" -not -path "src/api/*" -exec rustfmt --check --edition 2021 {} +

cargo_doc: ## Build the crate documentation
	cargo doc --no-deps

clean_generated_api: ## Clear the generated stubs in src/api
	rm -rf src/api

clean: ## Remove cargo build output, the packaged crate and the generation staging directory
	cargo clean
	rm -rf crate-dist ${PROTO_INPUT_DIR} build_check.txt

########################################################
#		Submodules

update_submodules: ## Initialize and update all submodules
	@echo "$(BLUE)[INFO]$(NC) START initializing submodules ..."
	git submodule update --init --recursive
	@echo "$(GREEN)[SUCCESS]$(NC) DONE initializing submodules"

checkout_defined_submodule_versions: ## Update submodule versions to the pins at the top of this Makefile
	@echo "$(BLUE)[INFO]$(NC) START checking out submodules ..."
	git -C ${ONDEWO_T2S_API_DIR} fetch --all
	git -C ${ONDEWO_T2S_API_DIR} checkout ${ONDEWO_T2S_API_GIT_BRANCH}
	git -C ${ONDEWO_PROTO_COMPILER_DIR} fetch --all
	git -C ${ONDEWO_PROTO_COMPILER_DIR} checkout ${ONDEWO_PROTO_COMPILER_GIT_BRANCH}
	@echo "$(GREEN)[SUCCESS]$(NC) DONE checking out submodules"

########################################################
#		Release

check_release_credentials: ## Fail loudly when the GitHub credential is unset or still a placeholder
# Read through the SHELL ($$VAR - this Makefile exports everything), never through make ($(VAR)),
# so the token is not interpolated into the recipe text and cannot reach the log.
#
# GITHUB_GH_TOKEN only: `release` deliberately does not upload to crates.io (release.yml does),
# so demanding CARGO_REGISTRY_TOKEN here would fail a release for a credential it never uses.
	@test -n "$$GITHUB_GH_TOKEN" -a "$$GITHUB_GH_TOKEN" != "ENTER_YOUR_TOKEN_HERE" || { echo "$(RED)[ERROR]$(NC) GITHUB_GH_TOKEN is not set - create one at https://github.com/settings/tokens, or run 'make ondewo_release', which reads it from ${DEVOPS_ACCOUNT_GIT}"; exit 1; }
	@echo "$(GREEN)[SUCCESS]$(NC) GITHUB_GH_TOKEN is set"

release: ## Automate the entire release process
	@echo "Start Release"
# Everything that can be refuted without touching origin is refuted FIRST. Both of these are
# otherwise only reached by push_to_gh, the last step - after the release branch and the tag have
# been pushed. A missing token would then leave an immovable tag on origin and `spc` would refuse
# every retry, because that branch and that tag now exist; an empty notes slice would publish an
# empty GitHub release instead of failing.
	make check_release_credentials
	make check_release_notes
	make build
	-make precommit_hooks_run_all_files
	git status
	make check_build
	git add Cargo.toml
	-git add Cargo.lock
# src/ carries BOTH the generated stubs (src/api) and every hand-written module beside them.
	git add src
	git add Makefile
	git add README.md
	git add RELEASE.md
# tests/ is not packaged, but leaving it out of the release commit means a regression test
# written alongside a fix never reaches the repository and CI never runs it.
	-git add tests
	git add ${ONDEWO_PROTO_COMPILER_DIR}
	git add ${ONDEWO_T2S_API_DIR}
	git status
	-git commit --no-verify -m "PREPARING FOR RELEASE ${ONDEWO_T2S_VERSION}"
	git push
	make create_release_branch
	make create_release_tag
	make push_to_gh
# The crates.io upload deliberately does NOT happen here. Pushing the tag above starts
# .github/workflows/release.yml, which publishes with the CARGO_REGISTRY_TOKEN repository secret;
# a second publisher in this recipe would race it and the loser would die on "crate version
# already uploaded". `make ondewo_publish_crate` is the manual path for when the workflow cannot
# run (no secret yet, GitHub Actions unavailable, or a re-publish after a fixed workflow).
	@echo "$(BLUE)[INFO]$(NC) Tag ${ONDEWO_T2S_VERSION} pushed - .github/workflows/release.yml publishes it to crates.io"
	@echo "Release Finished"

create_release_branch: ## Create Release Branch and push it to origin
	git checkout -b "release/${ONDEWO_T2S_VERSION}"
	git push -u origin "release/${ONDEWO_T2S_VERSION}"

create_release_tag: ## Create Release Tag and push it to origin
	git tag -a ${ONDEWO_T2S_VERSION} -m "release/${ONDEWO_T2S_VERSION}"
	git push origin ${ONDEWO_T2S_VERSION}

########################################################
#		GITHUB

push_to_gh: login_to_gh build_gh_release ## Logs into GitHub CLI and Releases
	@echo 'Released to Github'

login_to_gh: check_release_credentials ## Login to Github CLI with Access Token
	@echo $(GITHUB_GH_TOKEN) | gh auth login -p ssh --with-token

check_release_notes: ## Assert RELEASE.md carries an entry for ONDEWO_T2S_VERSION
# `gh release create -n ""` succeeds and publishes an EMPTY release, so an entry that was
# forgotten - or a heading whose wording drifted away from what CURRENT_RELEASE_NOTES greps for -
# is otherwise only noticed by whoever reads the release page afterwards.
	@notes="$(CURRENT_RELEASE_NOTES)"; \
	if [ -z "$$notes" ]; then \
		echo "$(RED)[ERROR]$(NC) RELEASE.md has no '## Release ONDEWO T2S Rust Client ${ONDEWO_T2S_VERSION}' entry"; \
		echo "        The GitHub release would be created with empty notes - add the entry first."; \
		exit 1; \
	fi; \
	echo "$(GREEN)[SUCCESS]$(NC) RELEASE.md has release notes for ${ONDEWO_T2S_VERSION}"

build_gh_release: check_release_notes ## Generate Github Release with CLI
	gh release create --repo $(GH_REPO) "$(ONDEWO_T2S_VERSION)" -n "$(CURRENT_RELEASE_NOTES)" -t "Release ${ONDEWO_T2S_VERSION}"

########################################################
#		CRATES.IO

check_crate_metadata: ## Assert Cargo.toml carries everything crates.io requires of a publishable crate
# `cargo publish --dry-run` already rejects an empty description/license/repository and a missing
# readme file, but it says nothing about keywords/categories (crates.io only recommends those) and
# it cannot see the server-side size limit. Keeping the whole list in one credential-free target
# means CI exercises every one of them on every push, instead of discovering them at release time.
	@for field in description license repository readme; do \
		grep -Eq "^$$field = \"[^\"]+\"" Cargo.toml || { echo "$(RED)[ERROR]$(NC) Cargo.toml carries no non-empty '$$field' - crates.io refuses the upload without it"; exit 1; }; \
	done
	@grep -Eq '^keywords = \[[^]]+\]' Cargo.toml || { echo "$(RED)[ERROR]$(NC) Cargo.toml carries no 'keywords' - the crate would be unfindable on crates.io"; exit 1; }
	@grep -Eq '^categories = \[[^]]+\]' Cargo.toml || { echo "$(RED)[ERROR]$(NC) Cargo.toml carries no 'categories' - the crate would be unfindable on crates.io"; exit 1; }
	@! grep -Eq '^[[:space:]]*publish[[:space:]]*=[[:space:]]*false' Cargo.toml || { echo "$(RED)[ERROR]$(NC) Cargo.toml sets 'publish = false' - the crate cannot be published at all"; exit 1; }
	@readme=`sed -n 's|^readme = "\(.*\)"|\1|p' Cargo.toml | head -n 1`; \
		test -s "$$readme" || { echo "$(RED)[ERROR]$(NC) the readme '$$readme' declared in Cargo.toml is missing or empty - crates.io renders it as the crate page"; exit 1; }
	@echo "$(GREEN)[SUCCESS]$(NC) Cargo.toml carries every field crates.io requires"

publish_crate_dry_run: check_crate_metadata ## Credential-free run of the whole packaging path, minus the upload (this is what CI runs)
# Everything the crates.io release does except the upload itself, and none of it needs a token.
# --allow-dirty throughout because this also runs over a working tree with freshly generated stubs;
# the real publish_crate deliberately has no such flag and refuses a dirty tree.
#
# First the exact file list crates.io would receive. The readme needs no assertion here: cargo
# always packages the file named by `readme`, even against an exclude entry, and
# check_crate_metadata has already proven that file exists and is non-empty.
	@echo "$(BLUE)[INFO]$(NC) Files that would be published:"
	cargo package --list --allow-dirty
# `cargo publish --dry-run` builds its tarball in a scratch directory and does not leave it behind,
# so the size limit is measured on the one `cargo package` writes. --no-verify keeps this step to
# the tarball alone (seconds); the compile that proves the packaged copy builds is the dry run below.
	cargo package --no-verify --allow-dirty
# Name the tarball from the manifest rather than globbing target/package: a cached build directory
# can still hold the .crate of an earlier version, and a glob would happily measure that one.
	@crate_file="target/package/`sed -n 's|^name = "\(.*\)"|\1|p' Cargo.toml | head -n 1`-`sed -n 's|^version = "\(.*\)"|\1|p' Cargo.toml | head -n 1`.crate"; \
		test -f "$$crate_file" || { echo "$(RED)[ERROR]$(NC) cargo package produced no $$crate_file"; exit 1; }; \
		crate_bytes=`wc -c < "$$crate_file" | tr -d ' '`; \
		test "$$crate_bytes" -le ${CRATE_MAX_BYTES} || { echo "$(RED)[ERROR]$(NC) $$crate_file is $$crate_bytes bytes, over the crates.io limit of ${CRATE_MAX_BYTES} - trim Cargo.toml's exclude list"; exit 1; }; \
		echo "$(GREEN)[SUCCESS]$(NC) $$crate_file is $$crate_bytes bytes (crates.io limit: ${CRATE_MAX_BYTES})"
	cargo publish --dry-run --allow-dirty

publish_crate: check_crate_metadata ## Publish the crate to crates.io (needs CARGO_REGISTRY_TOKEN)
# cargo reads CARGO_REGISTRY_TOKEN from the environment (this Makefile exports it), so the token
# never appears on a command line or in the build log. The guard below reads it through the SHELL
# ($$VAR), not through make ($(VAR)), so the token is not interpolated into the recipe text either.
	@test -n "$$CARGO_REGISTRY_TOKEN" -a "$$CARGO_REGISTRY_TOKEN" != "ENTER_HERE_YOUR_CARGO_REGISTRY_TOKEN" || { echo "$(RED)[ERROR]$(NC) CARGO_REGISTRY_TOKEN is not set - create one at https://crates.io/settings/tokens, or run 'make ondewo_publish_crate' to take it from the devops-accounts repo"; exit 1; }
	@echo "$(BLUE)[INFO]$(NC) Publishing ondewo-t2s-client ${ONDEWO_T2S_VERSION} to crates.io ..."
	cargo publish
	@echo "$(GREEN)[SUCCESS]$(NC) Published to crates.io"

package_crate: ## Package the crate locally (the same artifact `make publish_crate` uploads)
	cargo package

########################################################
#		DEVOPS-ACCOUNTS

ondewo_release: spc clone_devops_accounts run_release_with_devops ## Release with credentials from devops-accounts repo
	@rm -rf ${DEVOPS_ACCOUNT_GIT}

ondewo_publish_crate: clone_devops_accounts run_publish_crate_with_devops ## Publish to crates.io with the token from the devops-accounts repo
	@rm -rf ${DEVOPS_ACCOUNT_GIT}

clone_devops_accounts: ## Clones devops-accounts repo
	if [ -d $(DEVOPS_ACCOUNT_GIT) ]; then rm -Rf $(DEVOPS_ACCOUNT_GIT); fi
	git clone git@bitbucket.org:ondewo/${DEVOPS_ACCOUNT_GIT}.git

run_release_with_devops: ## Gets Credentials from devops-repo and run release command with them
# Only the GitHub token: `release` no longer uploads to crates.io (the release workflow does), so
# requiring account_cargo.env here would fail a release for a credential it does not use.
	$(eval info:= $(shell cat ${DEVOPS_ACCOUNT_DIR}/account_github.env | grep GITHUB_GH))
	@make release $(info)

run_publish_crate_with_devops: ## Gets the crates.io token from the devops-repo and runs the publish with it
# @-prefixed like every other line that carries a token: the expanded recipe line holds
# CARGO_REGISTRY_TOKEN=<token>, so an echoed line would put the token straight into the log.
	$(eval info:= $(shell cat ${DEVOPS_ACCOUNT_DIR}/account_cargo.env | grep CARGO_REGISTRY_TOKEN))
	@make publish_crate $(info)

spc: ## Checks if the Release Branch, Tag and crate version already exist
	$(eval filtered_branches:= $(shell git branch --all | grep "release/${ONDEWO_T2S_VERSION}"))
	$(eval filtered_tags:= $(shell git tag --list | grep "${ONDEWO_T2S_VERSION}"))
	$(eval cargo_version:= $(shell sed -n 's|^version = "\(.*\)"|\1|p' Cargo.toml | head -n 1))
	@if test "$(filtered_branches)" != ""; then echo "-- Test 1: Branch exists!!" & exit 1; else echo "-- Test 1: Branch is fine";fi
	@if test "$(filtered_tags)" != ""; then echo "-- Test 2: Tag exists!!" & exit 1; else echo "-- Test 2: Tag is fine";fi
	@if test "$(cargo_version)" != "${ONDEWO_T2S_VERSION}"; then echo "-- Test 3: Cargo.toml not updated!!" & exit 1; else echo "-- Test 3: Cargo.toml is fine";fi
