//! One-line decisions out of free text.
//!
//! The shared rule with the scripted shims: the model answers with exactly
//! `ACT <node> <action>` or `BYE`. Anything else is unusable, and an unusable
//! answer twice in a row ends the session rather than acts on a guess.

/// A usable decision, or the reason the text was not one.
pub fn parse_decision(text: &str) -> Option<Decision> {
    for line in text.lines() {
        let stripped = line.trim();
        if stripped == "BYE" {
            return Some(Decision::Bye);
        }
        if let Some(rest) = stripped.strip_prefix("ACT ") {
            let mut parts = rest.split_whitespace();
            if let (Some(node), Some(action)) = (parts.next(), parts.next()) {
                return Some(Decision::Act {
                    node: node.to_string(),
                    action: action.to_string(),
                });
            }
        }
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Act { node: String, action: String },
    Bye,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_decision_line_among_prose() {
        let text = "Sure, I'll toggle it.\nACT gift toggle\nDone.";
        assert_eq!(
            parse_decision(text),
            Some(Decision::Act {
                node: "gift".into(),
                action: "toggle".into(),
            })
        );
    }

    #[test]
    fn bye_is_a_decision() {
        assert_eq!(parse_decision("thanks\nBYE\n"), Some(Decision::Bye));
    }

    #[test]
    fn garbage_is_not_a_decision() {
        assert_eq!(parse_decision("Let me think about that..."), None);
        assert_eq!(parse_decision("ACT"), None);
        assert_eq!(parse_decision("ACT gift"), None);
        assert_eq!(parse_decision(""), None);
    }
}
