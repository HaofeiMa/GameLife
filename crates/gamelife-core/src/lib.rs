pub mod capture;
pub mod document;
pub mod early_start;
pub mod feel;
pub mod r#const;
pub mod format;
pub mod heartbeat;
pub mod hint;
pub mod judge;
pub mod ledger;
pub mod observe;
pub mod policy;
pub mod quest;
pub mod shop;
pub mod streak;
pub mod task;
pub mod task_ai;
pub mod task_parse;
pub mod time;
pub mod types;
pub mod url;
pub mod vision_ctx;
pub mod weekly;

pub use capture::{CaptureStatus, capture_on_resume, schedule_capture};
pub use early_start::{early_start_anchor, early_start_coins_for_local_secs};
pub use feel::{FeelNotice, LedgerSlice, coalesce_feel_events};
pub use r#const::*;
pub use format::format_estimated_minutes;
pub use heartbeat::heartbeat_unobserved;
pub use hint::{hint_sample, is_grounded_core_sample};
pub use judge::{
    Dominant, JudgeInput, JudgeOutput, SlotEvidence, VisionMatchContext, VisionParseError,
    VisionResult, analyze_slot_evidence, credited_core_spans, judge_slot, parse_vision_json,
    sum_unsure_spans,
};
pub use ledger::{
    CHORE_COIN_TICK_SECS, CHORE_XP_TICK_SECS, SIDE_COIN_TICK_SECS, SIDE_XP_TICK_SECS, RewardEvent,
    admin_xp_key, support_xp_key, tick_keys_for_credited, tick_keys_for_discount,
};
pub use observe::{Span, SpanKind, observed_seconds, spans_for_slot};
pub use policy::{
    CategoryGuides, Policy, builtin_never_capture, builtin_side_project_rules,
    default_distraction_rules, default_reading_apps, default_trusted_apps, default_v01,
    matches_app_identity, never_capture_removable, nonempty_guides, truncate_guide,
};
pub use quest::{
    MAX_EVIDENCE, MAX_QUESTS, MIN_EVIDENCE_CHARS, QuestDraft, QuestListError,
    matched_quest_index, normalize_evidence, normalize_quest_list, parse_quest_versions_json,
    quest_list_has_evidence, vision_quest_label,
};
pub use shop::{
    RedeemError, Wish, WishError, WishKind, can_start_entertainment,
    entertainment_remaining_secs, has_entertainment_timer, tray_entertainment_minutes,
    validate_redeem, validate_wish, xp_shop_unlocked,
};
pub use streak::{
    DayOutcome, FREEZE_PER_MONTH, can_use_freeze, freeze_month_key, freeze_quota_used,
    new_milestones, recompute_streak,
};
pub use task::{
    ListRole, Task, TaskList, TaskListError, TaskRange, TaskSnapshot, TimedTask, MAX_JUDGMENT_TASKS,
    PRESET_CHORE_ID, PRESET_LONGTERM_ID, PRESET_MAINLINE_ID, PRESET_SIDE_ID, align_range,
    in_judgment_set, judgment_tasks, match_role_alias, parse_task_snapshot_json, preset_lists,
    role_from_hashtag, snapshot_evidence_quests, snapshot_of, ticktick_judgment_set,
    ticktick_snapshot_id, tokenize_title, validate_lists,
};
pub use task_parse::{ParseContext, ParsedTask, parse_task_line};
pub use task_ai::{
    apply_category_match, apply_task_match, parse_category_match_json, parse_task_match_json,
    payout_base_seconds, CategoryMatch, CategoryMatchError, TaskMatch, TaskMatchError,
    TASK_MATCH_MIN,
};
pub use time::{is_weekday, slot_end_exclusive, slot_start};
pub use types::{ActivitySeconds, Hint, Quest, Sample};
pub use url::{host_is_research, optional_stripped_url, strip_url_query_fragment, url_host};
pub use document::normalize_document_path;
pub use vision_ctx::{
    ActivitySummary, CaptureContext, HintSeconds, SanitizedVisionContext, VisionContext,
    VisionPrivacyError, WindowShare, build_vision_prompt, format_span_secs,
    is_protected_frontmost, sanitize_vision_context, activity_summary_for_vision,
};
pub use weekly::sum_activity;
