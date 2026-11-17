//! Guard against a whole class of dead controls.
//!
//! GPUI dispatches a click by element id, so an element built inside a loop whose
//! id is a constant is inert for every member but one. This test reads the crate
//! sources and fails if a loop contains a constant `.id("...")` or
//! `.on_press("...")` again.

use std::path::{Path, PathBuf};

fn sources(directory: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("read source directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            found.push(path);
        }
    }
}

/// The body of every `for ... in ... {` block, found by matching braces.
fn loop_bodies(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut bodies = Vec::new();
    let mut line_start = 0;
    while line_start < bytes.len() {
        let line_end = source[line_start..]
            .find('\n')
            .map(|offset| line_start + offset)
            .unwrap_or(bytes.len());
        let line = &source[line_start..line_end];
        let trimmed = line.trim_start();
        if trimmed.starts_with("for ") && trimmed.contains(" in ") {
            if let Some(offset) = line.find('{') {
                let open = line_start + offset;
                let mut depth = 0i32;
                let mut cursor = open;
                while cursor < bytes.len() {
                    match bytes[cursor] {
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    cursor += 1;
                }
                bodies.push(source[open..cursor.min(bytes.len())].to_string());
            }
        }
        line_start = line_end + 1;
    }
    bodies
}

#[test]
fn no_loop_reuses_one_element_id() {
    let mut files = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    let mut failures = Vec::new();
    for file in files {
        let source = std::fs::read_to_string(&file).expect("read source");
        for body in loop_bodies(&source) {
            for needle in [".id(\"", ".on_press(\n", ".on_click("] {
                if !body.contains(needle) {
                    continue;
                }
                if needle == ".id(\"" {
                    failures.push(format!(
                        "{}: a loop uses a constant `.id(\"...\")`; derive the id from the item",
                        file.display()
                    ));
                }
                if needle == ".on_press(\n" {
                    // A literal id right after the call is the constant form.
                    for (position, _) in body.match_indices(".on_press(") {
                        let rest = body[position + ".on_press(".len()..].trim_start();
                        if rest.starts_with('"') {
                            failures.push(format!(
                                "{}: a loop uses a constant `.on_press(\"...\")`; derive the id from the item",
                                file.display()
                            ));
                        }
                    }
                }
                if needle == ".on_click(" {
                    let has_id = body.contains(".id(");
                    if !has_id {
                        failures.push(format!(
                            "{}: a loop registers `.on_click` on an element without an id",
                            file.display()
                        ));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "element id problems:\n{}",
        failures.join("\n")
    );
}
