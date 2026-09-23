//! INT-09: the plugin interface (`wit/kivo-plugin.wit`) and KIVO's own tool schema must stay the
//! same shape, so every built-in tool could be a plugin's tool. These tests read the WIT draft and
//! check it against `ToolSpec` and against every tool KIVO registers.

#[cfg(test)]
mod tests {
    use crate::testing::rig;
    use kivo_core::tool::{CapabilityTier, Reversibility, Risk, SideEffect};
    use std::collections::BTreeSet;

    const WIT: &str = include_str!("../wit/kivo-plugin.wit");

    fn kebab(camel: &str) -> String {
        let mut out = String::new();
        for c in camel.chars() {
            if c.is_ascii_uppercase() {
                out.push('-');
                out.push(c.to_ascii_lowercase());
            } else {
                out.push(c);
            }
        }
        out
    }

    /// The body of `<kind> <name> { … }` in the WIT file.
    fn block(kind: &str, name: &str) -> String {
        let start = WIT
            .find(&format!("{kind} {name} {{"))
            .unwrap_or_else(|| panic!("no {kind} {name}"));
        let rest = &WIT[start..];
        let open = rest.find('{').unwrap();
        // A one-line block closes on its line; a multi-line one at the next line that is only
        // `}` (comments may contain braces).
        let first_line_end = rest.find('\n').unwrap_or(rest.len());
        let close = match rest[..first_line_end].rfind('}') {
            Some(c) if c > open => c,
            _ => rest
                .find("\n    }")
                .unwrap_or_else(|| rest.find('}').unwrap()),
        };
        rest[open + 1..close].to_owned()
    }

    fn record_fields(name: &str) -> BTreeSet<String> {
        block("record", name)
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with("//") && l.contains(':'))
            .map(|l| l.split(':').next().unwrap().trim().to_owned())
            .collect()
    }

    fn enum_cases(name: &str) -> BTreeSet<String> {
        block("enum", name)
            .split(',')
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty())
            .collect()
    }

    fn serde_names<T: serde::Serialize>(values: &[T]) -> BTreeSet<String> {
        values
            .iter()
            .map(|v| kebab(serde_json::to_value(v).unwrap().as_str().unwrap()))
            .collect()
    }

    #[test]
    fn the_wit_tool_spec_is_the_tool_spec() {
        let r = rig();
        let spec = r.tools[0].spec().clone();
        let fields: BTreeSet<String> = serde_json::to_value(&spec)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .map(|k| kebab(k))
            .collect();
        assert_eq!(record_fields("tool-spec"), fields);
    }

    #[test]
    fn the_wit_enums_are_the_rust_enums() {
        use Risk::{High, Low, Medium, Safe};
        assert_eq!(enum_cases("risk"), serde_names(&[Safe, Low, Medium, High]));
        use SideEffect as S;
        assert_eq!(
            enum_cases("side-effect"),
            serde_names(&[
                S::None,
                S::LocalRead,
                S::LocalWrite,
                S::Destructive,
                S::ExternalComms,
                S::Financial,
                S::SecuritySensitive
            ])
        );
        use CapabilityTier as T;
        assert_eq!(
            enum_cases("capability-tier"),
            serde_names(&[
                T::Native,
                T::OsApi,
                T::AppCli,
                T::Uia,
                T::BrowserDom,
                T::A11y,
                T::Vision,
                T::Input
            ])
        );
        use Reversibility as R;
        assert_eq!(
            enum_cases("reversibility"),
            serde_names(&[R::NotApplicable, R::Undoable, R::Irreversible])
        );
    }

    /// Every registered tool fits the plugin shape: a namespaced id, a JSON Schema object for its
    /// arguments, a kebab-case capability, a title and a description.
    #[test]
    fn every_tool_fits_the_plugin_shape() {
        let r = rig();
        for tool in &r.tools {
            let s = tool.spec();
            assert!(s.id.contains('.') && !s.id.contains(' '), "{}", s.id);
            assert_eq!(s.params["type"], "object", "{}", s.id);
            let cap = serde_json::to_value(s.capability).unwrap();
            assert!(
                cap.as_str()
                    .unwrap()
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-')
            );
            assert!(
                !s.description.is_empty() && !s.title.starts_with("tool."),
                "{}",
                s.id
            );
            assert!(s.timeout_ms > 0 && !s.side_effects.is_empty(), "{}", s.id);
        }
    }
}
