pub mod app_stats;
pub mod capture;
pub mod r#const;
pub mod document;
pub mod early_start;
pub mod feel;
pub mod format;
pub mod heartbeat;
pub mod hint;
pub mod judge;
pub mod ledger;
pub mod observe;
pub mod policy;
pub mod quest;
pub mod reports;
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

pub use app_stats::{
    accumulate_sample, deltas_from_slot, hint_bucket, is_lock_screen_app, AppHintSecs, DayStatDelta,
};
pub use capture::{capture_on_resume, schedule_capture, CaptureStatus};
pub use document::{looks_like_work_path, normalize_document_path};
pub use early_start::{early_start_anchor, early_start_coins_for_local_secs};
pub use feel::{coalesce_feel_events, FeelNotice, LedgerSlice};
pub use format::format_estimated_minutes;
pub use heartbeat::heartbeat_unobserved;
pub use hint::{hint_sample, is_grounded_core_sample};
pub use judge::{
    analyze_slot_evidence, credited_core_spans, judge_slot, parse_vision_json, sum_unsure_spans,
    Dominant, JudgeInput, JudgeOutput, SlotEvidence, VisionMatchContext, VisionParseError,
    VisionResult, VISION_CONFIDENCE_MIN,
};
pub use ledger::{
    admin_xp_key, support_xp_key, tick_keys_for_credited, tick_keys_for_discount, RewardEvent,
    CHORE_COIN_TICK_SECS, CHORE_XP_TICK_SECS, SIDE_COIN_TICK_SECS, SIDE_XP_TICK_SECS,
};
pub use observe::{observed_seconds, spans_for_slot, Span, SpanKind};
pub use policy::{
    builtin_never_capture, builtin_side_project_rules, default_distraction_rules,
    default_reading_apps, default_trusted_apps, default_v01, matches_app_identity,
    never_capture_removable, nonempty_guides, truncate_guide, CategoryGuides, Policy,
};
pub use quest::{
    matched_quest_index, normalize_evidence, normalize_quest_list, parse_quest_versions_json,
    quest_list_has_evidence, vision_quest_label, QuestDraft, QuestListError, MAX_EVIDENCE,
    MAX_QUESTS, MIN_EVIDENCE_CHARS,
};
pub use r#const::*;
pub use reports::{distraction_runs, first_core_hour, hit_rate, month_heat_cell, wow_delta};
pub use shop::{
    can_start_entertainment, entertainment_remaining_secs, has_entertainment_timer,
    tray_entertainment_minutes, validate_redeem, validate_wish, xp_shop_unlocked, RedeemError,
    Wish, WishError, WishKind,
};
pub use streak::{
    can_use_freeze, freeze_month_key, freeze_quota_used, new_milestones, recompute_streak,
    settle_outcome, streak_at_risk, DayOutcome, FREEZE_PER_MONTH,
};
pub use task::{
    align_range, can_delete_list, clear_schedule, in_judgment_set, is_preset_list_id,
    judgment_tasks, match_role_alias, match_task_role, move_range_to_day, next_occurrence,
    notes_ok, parse_list_role_strict, parse_task_snapshot_json, preset_lists, remind_offsets_ok,
    resize_range, role_from_hashtag, schedule_from_drop, select_prompt_snapshots,
    snapshot_evidence_quests, snapshot_of, snapshots_for_day, snapshots_open, spawn_after_complete,
    ticktick_day_list, ticktick_judgment_set, ticktick_listed_on_day, ticktick_overlapping_count,
    ticktick_snapshot_id, tokenize_title, validate_lists, ListRole, MatchRole, RepeatRule,
    ResizeEdge, Task, TaskList, TaskListError, TaskRange, TaskSnapshot, TimedTask,
    ALLOWED_REMIND_OFFSETS, MAX_JUDGMENT_TASKS, MAX_NOTES_BYTES, PRESET_CHORE_ID, PRESET_LIST_IDS,
    PRESET_LONGTERM_ID, PRESET_MAINLINE_ID, PRESET_SIDE_ID, UNSCHEDULED_DROP_SECS,
};
pub use task_ai::{
    apply_category_match, apply_task_match, parse_category_match_json, parse_task_match_json,
    payout_base_seconds, settle_from_text_ai, CategoryMatch, CategoryMatchError, TaskMatch,
    TaskMatchError, TASK_MATCH_MIN,
};
pub use task_parse::{parse_task_line, ParseContext, ParseSpan, ParsedTask};
pub use time::{is_weekday, slot_end_exclusive, slot_start};
pub use types::{ActivitySeconds, Hint, Quest, Sample};
pub use url::{host_is_research, optional_stripped_url, strip_url_query_fragment, url_host};
pub use vision_ctx::{
    activity_summary_for_vision, build_vision_prompt, format_span_secs, is_protected_frontmost,
    sanitize_vision_context, ActivitySummary, CaptureContext, HintSeconds, SanitizedVisionContext,
    VisionContext, VisionPrivacyError, WindowShare,
};
pub use weekly::sum_activity;
