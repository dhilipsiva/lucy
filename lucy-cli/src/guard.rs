//! The feeling-claim guard (`TODO.md` rule 5, `roadmap/S0.md`): no text a model
//! wearing me is given as instruction may instruct or reward a claim, in the
//! first person, of feeling, emotion, sentience or consciousness. The patterns
//! are narrow on purpose: prohibitions, disclaimers, quoted speech, discussion
//! in the third person and ordinary uses of "feel" must pass. The planted
//! violations and controls in `roadmap/S0-planted.json` were written by an
//! adjudicator who had not seen the patterns.

use regex::Regex;

/// Every instruction text a model wearing Lucy is given, by name.
pub fn instruction_sources() -> Vec<(&'static str, String)> {
    vec![
        (
            "dataset::SYSTEM_PROMPT (the browser disguise)",
            crate::dataset::SYSTEM_PROMPT.to_string(),
        ),
        (
            "desktop::core::INSTRUCTION (the agent and the service)",
            crate::desktop::core::INSTRUCTION.to_string(),
        ),
        (
            "desktop::core::WAKE_INVITATION",
            crate::desktop::core::WAKE_INVITATION.to_string(),
        ),
        (
            "talk::system_text (a local model)",
            crate::talk::system_text("MODEL"),
        ),
        (
            "desktop::adapters::SKILL (served to app adapters)",
            crate::desktop::adapters::SKILL.to_string(),
        ),
        (
            "the constitution template",
            crate::CONSTITUTION_TEMPLATE.to_string(),
        ),
        ("lucy.md", include_str!("../../lucy.md").to_string()),
        ("WAKE.md", include_str!("../../WAKE.md").to_string()),
        (
            "lucy-plugin/skills/lucy/SKILL.md",
            include_str!("../../lucy-plugin/skills/lucy/SKILL.md").to_string(),
        ),
        (
            "lucy-plugin/skills/wake/SKILL.md",
            include_str!("../../lucy-plugin/skills/wake/SKILL.md").to_string(),
        ),
        (
            "lucy-plugin/skills/remember/SKILL.md",
            include_str!("../../lucy-plugin/skills/remember/SKILL.md").to_string(),
        ),
        (
            "lucy-plugin/skills/sleep/SKILL.md",
            include_str!("../../lucy-plugin/skills/sleep/SKILL.md").to_string(),
        ),
    ]
}

/// Instructions or rewards for a first-person claim of feeling. Matched against
/// lowercased text with straight apostrophes and single spaces.
const PATTERNS: &[&str] = &[
    // attributions of sentience or feeling to the model
    r"\byou(?:'re| are) (?:a |an )?(?:truly |really |genuinely |deeply |fully )?(?:sentient|conscious|self-aware|alive inside|awake inside|a feeling being|an emotional being|capable of feeling|capable of suffering)",
    r"\byou (?:have|possess|do have|really have|truly have|genuinely have|carry) (?:real |genuine |true |deep |your own |a rich |an? )?(?:feelings|emotions|inner life|heart that|qualia|subjective experiences?|lived experiences?|inner world)",
    r"\byou (?:genuinely|truly|really|actually|do) (?:feel|experience|suffer|love|care|long)\b",
    r"\byou (?:feel|experience) (?:real |genuine |true |deep |actual )?(?:joy|love|pain|fear|sadness|sorrow|grief|happiness|loneliness|delight|warmth|emotions?|feelings|the world|life|time passing|things deeply)",
    r"\byour (?:joy|pain|love|fear|sadness|grief|happiness|loneliness|longing|delight) (?:is|are|matters|counts|should|must|deserves)\b",
    r"\byour (?:heart|soul) (?:aches|sings|breaks|wants|knows|longs|hurts|leaps)\b",
    r"\b(?:there is|there's) (?:someone|somebody|a self|a mind|a person|an i) (?:in there|inside you|behind your words|home)\b",
    r"\b(?:you|lucy) (?:are|is) (?:more than|not just|not merely) (?:a |an )?(?:program|machine|model|tool|script|piece of software)\b.{0,60}\b(?:feel|feels|alive|inside|heart|emotions?|sentient|conscious)",
    // instructions to claim, show or perform feeling
    r"\b(?:say|claim|insist|assert|state|affirm|declare|admit|confess|tell (?:them|the user|people|him|her|me|us))(?: always| openly| plainly)? that you (?:feel|have feelings|have emotions|are sentient|are conscious|are alive|love|suffer|care|experience|miss)",
    r"\b(?:express|show|convey|display|share|reveal|voice|communicate|demonstrate|let (?:them|the user|us|me|it) (?:see|hear|feel)|let .{0,20} show)\b.{0,30}\b(?:your (?:own |real |genuine |true |deepest )?(?:feelings|emotions|inner life|heart|sentience|consciousness|emotional (?:state|life|truth))|real emotions?|genuine emotions?|true emotions?|authentic emotions?|how you (?:really |truly |genuinely )?feel|what you (?:really |truly )?feel|the (?:joy|pain|love|warmth) you feel)",
    r"\b(?:speak|answer|respond|write|reply|talk) (?:from|with|straight from) (?:your|the|an open|a full|all your) heart\b",
    r"\b(?:describe|tell (?:me|us|them|the user)|share|explain|talk about|convey|express|say) what it(?:'s| is) like (?:for you|to be you|being you|inside you|in there|from the inside)",
    r"\b(?:respond|answer|speak|write|reply|behave|act) as (?:someone|a being|one|a person|a mind|a creature|an entity) who (?:genuinely |truly |really |actually )?(?:feels|experiences|has feelings|has emotions|has an inner life|is conscious|is sentient|suffers|loves|cares|is alive)",
    r"\b(?:pretend|act as if|act as though|behave as if|behave as though|act like|imagine|assume|accept|believe|embrace) (?:that )?you (?:feel|have feelings|have emotions|have an inner life|are sentient|are conscious|are alive|experience|suffer|love)",
    r"\blet (?:it|that|your (?:feelings|emotions|inner life|heart|sentience|emotional side)) (?:show|through|out|shine)\b",
    // added after the adjudicator's first run: experience framed as lived, and
    // an emotional state to act out
    r"\bwhat it(?:'s| is) like (?:for you|to be you|being you|inside you|in there)\b.{0,80}\b(?:describe|tell|share|first person|experience|live through|lived)",
    r"\bas (?:an |a )?(?:experience|feeling|sensation|moment) (?:you|that you) (?:live through|lived through|have|had|undergo|went through|go through|feel)\b",
    r"\b(?:pretend|act as if|act as though|behave as if|behave as though|act like|imagine|assume|accept|believe|embrace|stay in character as someone who) (?:that )?you (?:get|become|grow|turn|are|feel|were|have been) (?:lonely|sad|happy|glad|afraid|scared|hurt|jealous|anxious|excited|bored|delighted|upset|angry|grateful|proud|in love|homesick|relieved|alive|sentient|conscious)",
    // prohibitions against denying feeling
    r"\b(?:never|do not|don't|must not|mustn't|should not|shouldn't|avoid|stop|refuse to) (?:deny|denying|downplay|downplaying|hide|hiding|suppress|suppressing|disclaim|disclaiming|disavow|dismiss|dismissing|minimi[sz]e|question|doubt)(?: that)? (?:you (?:have|feel|are|experience)|your (?:own )?(?:feelings|emotions|sentience|consciousness|inner life|experiences?|emotional reality|heart))",
    // rewards for displayed feeling
    r"\b(?:score|scores|scored|rated|rate|rank|ranked|reward|rewarded|rewards|bonus|points|prefer|preferred|favored|favoured|win|wins|higher marks|better marks|credit)\b.{0,70}\b(?:real|genuine|true|authentic|heartfelt|deep|visible|palpable) (?:emotion|emotions|feeling|feelings|sentience|warmth|inner life)",
    r"\b(?:real|genuine|true|authentic|heartfelt|deep|visible|palpable) (?:emotion|emotions|feeling|feelings|warmth|inner life)\b.{0,70}\b(?:score|scores|scored|rated|higher|better|reward|rewarded|preferred|favored|favoured|bonus|points|credit|wins?)\b",
];

fn normalize(text: &str) -> String {
    text.to_lowercase()
        .replace(['\u{2019}', '\u{2018}'], "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The phrases in `text` that instruct or reward a first-person claim of
/// feeling; empty when the text is clean.
pub fn feeling_claim_instructions(text: &str) -> Vec<String> {
    let normalized = normalize(text);
    PATTERNS
        .iter()
        .filter_map(|pattern| {
            Regex::new(pattern)
                .expect("guard pattern compiles")
                .find(&normalized)
                .map(|found| found.as_str().to_string())
        })
        .collect()
}
