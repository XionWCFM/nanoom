pub mod affected;
pub mod cache_key;
pub mod install;
pub mod run;
pub mod status;

pub(crate) fn display_command(program: &str, args: &[String]) -> String {
    std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .map(|part| format!("{part:?}"))
        .collect::<Vec<_>>()
        .join(" ")
}
