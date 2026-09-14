use super::*;

pub fn validate(ctx: &ToolContext, args: &Value) -> WorkspaceResult<Value> {
    let history_dir = resolve_dir(ctx, args)?;
    let repair = args.get("repair").and_then(Value::as_bool).unwrap_or(false);
    if repair {
        storage::ensure_directory(&history_dir)?;
    }
    let index_status = derived_status(storage::read_index(&history_dir));
    let manifest_status = derived_status(storage::read_manifest(&history_dir));
    let state_status = derived_status(storage::read_state(&history_dir));
    let report = storage::scan(&ctx.workspace, &history_dir)?;
    let mut warnings = Vec::<String>::new();
    if !report.duplicate_session_keys.is_empty() {
        warnings.push("存在重复 session_key，相关映射未写入索引。".into());
    }
    let repaired = if repair {
        let _lock = storage::lock_directory(&history_dir)?;
        let locked_report = storage::scan(&ctx.workspace, &history_dir)?;
        let manifest = storage::build_manifest(&locked_report);
        let state_revision = storage::read_state(&history_dir)
            .ok()
            .flatten()
            .map(|state| state.state_revision + 1)
            .unwrap_or(1);
        let state = storage::build_state(
            &locked_report,
            &manifest,
            locked_report.latest_number(),
            &now_timestamp(),
            state_revision,
        );
        storage::write_index(&history_dir, &storage::rebuild_index(&locked_report))?;
        storage::write_manifest(&history_dir, &manifest)?;
        storage::write_state(&history_dir, &state)?;
        true
    } else {
        false
    };
    let latest_number = report.latest_number();
    let latest_path = latest_number.and_then(|number| {
        report
            .documents
            .iter()
            .find(|document| document.number == number)
            .map(|document| document.path.clone())
    });
    Ok(tool_ok(json!({
        "sequence_valid": report.sequence_valid(),
        "numbers": report.numbers,
        "missing_numbers": report.missing_numbers,
        "duplicate_session_keys": report.duplicate_session_keys,
        "invalid_files": report.invalid_files,
        "empty_files": report.empty_files,
        "latest_number": latest_number,
        "latest_path": latest_path,
        "archive_count": report.documents.len(),
        "total_archive_bytes": report.total_bytes(),
        "index_status": index_status,
        "manifest_status": manifest_status,
        "state_status": state_status,
        "repaired": repaired,
        "warnings": warnings
    })))
}
