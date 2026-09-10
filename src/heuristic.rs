use crate::types::Tier;

/// Cheap auto-tier when `RememberRequest.tier` is omitted.
///
/// Profile: durable self-facts. Episodic: day/event language. Else working.
pub fn infer_tier(text: &str) -> Tier {
    let t = text.to_lowercase();
    const PROFILE: &[&str] = &[
        "i am ",
        "i'm ",
        "my name",
        "i prefer",
        "i always",
        "i never",
        "prefers ",
        "lives in",
        "timezone",
        "allergic",
        "i work",
        "my job",
        "user prefers",
        "user always",
    ];
    const EPISODIC: &[&str] = &[
        "today ",
        "today,",
        "yesterday",
        "this morning",
        "this afternoon",
        "last night",
        "happened",
        "we went",
        "i went",
        "i did ",
        "i met ",
        "this week",
    ];
    if PROFILE.iter().any(|p| t.contains(p)) {
        return Tier::Profile;
    }
    if EPISODIC.iter().any(|p| t.contains(p)) {
        return Tier::Episodic;
    }
    Tier::Working
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facts_go_to_profile() {
        assert_eq!(infer_tier("I prefer dark mode"), Tier::Profile);
    }

    #[test]
    fn events_go_to_episodic() {
        assert_eq!(infer_tier("Today I hiked Mount Tam"), Tier::Episodic);
    }

    #[test]
    fn default_working() {
        assert_eq!(
            infer_tier("scratch note about rust generics"),
            Tier::Working
        );
    }
}
