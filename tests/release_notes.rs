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

//! The GitHub release body is sliced out of RELEASE.md by the Makefile's `CURRENT_RELEASE_NOTES`.
//!
//! The slice starts at `Release ONDEWO T2S Rust Client <version>` and ends at the next
//! `*****` line. A heading spelled any other way gives an empty slice, and `gh release create -n ""`
//! then publishes a release without notes and without an error; a range that ended at the first
//! `**` instead would cut the notes at their first bold span.

const RELEASE_NOTES: &str = include_str!("../RELEASE.md");
const MAKEFILE: &str = include_str!("../Makefile");
const HEADING_PREFIX: &str = "## Release ONDEWO T2S Rust Client ";

fn is_separator(line: &str) -> bool {
    line.starts_with("*****")
}

/// Reproduce the Makefile's perl range `/Release ONDEWO T2S Rust Client <v>/../^\*{5}/`.
fn release_notes_slice(version: &str) -> Vec<&'static str> {
    let lines: Vec<&str> = RELEASE_NOTES.lines().collect();
    let needle = format!("Release ONDEWO T2S Rust Client {version}");
    let start = lines
        .iter()
        .position(|line| line.contains(&needle))
        .unwrap_or_else(|| panic!("RELEASE.md has no '{needle}' heading"));
    let end = (start + 1..lines.len())
        .find(|&index| is_separator(lines[index]))
        .unwrap_or_else(|| panic!("the section of {version} does not end at a ***** separator"));
    lines[start..=end].to_vec()
}

fn headings() -> Vec<&'static str> {
    RELEASE_NOTES
        .lines()
        .filter(|line| line.starts_with("## "))
        .collect()
}

fn version_of(heading: &str) -> &str {
    heading.strip_prefix(HEADING_PREFIX).unwrap()
}

#[test]
fn the_makefile_slices_the_heading_this_test_checks() {
    assert!(MAKEFILE.contains(
        "perl -ne 'print if /Release ONDEWO T2S Rust Client ${ONDEWO_T2S_VERSION}/../^\\*{5}/'"
    ));
    assert!(MAKEFILE.contains("-n \"$(CURRENT_RELEASE_NOTES)\""));
}

#[test]
fn every_release_heading_uses_the_spelling_the_makefile_slices() {
    let headings = headings();
    assert!(!headings.is_empty());
    for heading in &headings {
        let version = heading.strip_prefix(HEADING_PREFIX);
        assert!(
            version.is_some_and(|version| {
                let parts: Vec<&str> = version.split('.').collect();
                parts.len() == 3
                    && parts
                        .iter()
                        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            }),
            "unexpected heading spelling: {heading:?}"
        );
    }
    let mut versions: Vec<&str> = headings.iter().map(|heading| version_of(heading)).collect();
    versions.sort_unstable();
    versions.dedup();
    assert_eq!(versions.len(), headings.len(), "a version has two sections");
}

#[test]
fn every_section_ends_at_its_separator_and_holds_only_its_own_notes() {
    for heading in headings() {
        let slice = release_notes_slice(version_of(heading));
        assert_eq!(slice[0], heading);
        assert!(is_separator(slice[slice.len() - 1]));
        assert!(
            slice[1..].iter().all(|line| !line.starts_with("## ")),
            "the section of {heading:?} runs into the next one"
        );
        let body: Vec<&&str> = slice[1..slice.len() - 1]
            .iter()
            .filter(|line| !line.trim().is_empty())
            .collect();
        assert!(!body.is_empty(), "the section of {heading:?} is empty");
    }
}

#[test]
fn the_current_version_has_non_empty_release_notes() {
    let version = MAKEFILE
        .lines()
        .find_map(|line| line.strip_prefix("ONDEWO_T2S_VERSION="))
        .expect("the Makefile sets ONDEWO_T2S_VERSION")
        .trim();
    let slice = release_notes_slice(version);
    let body: Vec<&&str> = slice[1..slice.len() - 1]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert!(body.len() > 1, "{version}: {body:?}");
}
