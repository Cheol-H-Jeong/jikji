pub(crate) fn print_json(value: &impl serde::Serialize) -> jikji_core::Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|source| jikji_core::json_error("<stdout>", source))?;
    println!("{text}");
    Ok(())
}

pub(crate) fn print_json_compact(value: &impl serde::Serialize) -> jikji_core::Result<()> {
    let text = serde_json::to_string(value)
        .map_err(|source| jikji_core::json_error("<stdout>", source))?;
    println!("{text}");
    Ok(())
}
