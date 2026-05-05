use std::fs;

use anyhow::{Context, Result};

pub(crate) fn patch_get_version_function(path: &str, version: &str) -> Result<()> {
    let content = fs::read_to_string(path).with_context(|| format!("failed reading {path}"))?;
    let mut lines: Vec<String> = content.lines().map(ToString::to_string).collect();

    let Some(index) = lines
        .iter()
        .position(|line| line.trim_end() == "def get_version():")
    else {
        return Ok(());
    };

    if lines.len() < index + 4 {
        return Ok(());
    }

    lines.splice(
        index..index + 4,
        [
            "def get_version():".to_string(),
            format!("    return '{version}'"),
        ],
    );

    let mut rewritten = lines.join("\n");
    rewritten.push('\n');
    fs::write(path, rewritten).with_context(|| format!("failed writing {path}"))?;
    Ok(())
}

pub(crate) fn patch_torch_load_single_line(path: &str) -> Result<()> {
    let content = fs::read_to_string(path).with_context(|| format!("failed reading {path}"))?;
    let mut replaced_any = false;
    let mut patched = Vec::with_capacity(content.lines().count());

    for line in content.lines() {
        let mut current = line.to_string();
        let mut search_from = 0usize;

        loop {
            let Some(relative_start) = current[search_from..].find("torch.load(") else {
                break;
            };
            let start = search_from + relative_start;
            let open_paren = start + "torch.load".len();
            let rest = &current[open_paren + 1..];
            let Some(close_rel) = rest.find(')') else {
                break;
            };

            let close_idx = open_paren + 1 + close_rel;
            let args = &current[open_paren + 1..close_idx];

            if args.contains("weights_only=") {
                search_from = close_idx + 1;
                continue;
            }

            current.insert_str(close_idx, ", weights_only=False");
            replaced_any = true;
            search_from = close_idx + ", weights_only=False".len() + 1;
        }

        patched.push(current);
    }

    if !replaced_any {
        return Ok(());
    }

    let mut rewritten = patched.join("\n");
    rewritten.push('\n');
    fs::write(path, rewritten).with_context(|| format!("failed writing {path}"))?;

    Ok(())
}
