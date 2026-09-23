//! TypeScript types for the UI, generated from the Rust IPC types (ARCHITECTURE §3, ARCH-18).
//!
//! Check (CI):   cargo test -p kivo-ipc --features ts --test ts_bindings
//! Regenerate:   KIVO_WRITE_TS=1 cargo test -p kivo-ipc --features ts --test ts_bindings
#![cfg(feature = "ts")]

use kivo_core::capability::{Badge, Capability};
use kivo_core::config::{
    CompanionStyle, IslandSize, IslandSpot, MotionPref, OverlayPosition, PermissionMode, Preset,
    SoundCue, SoundSet,
};
use kivo_core::event::{
    CancelReason, DeviceKind, EventKind, IntentPath, PermissionDecision, ProviderEvent, StepStatus,
    SystemEvent, TaskEvent, ToolEvent, TtsStopReason, TurnEvent, TurnSource, UiEvent, VoiceEvent,
};
use kivo_core::routine::{Routine, RoutineStep, Trigger, VarDef, VarKind};
use kivo_core::task::{
    Criterion, Notify, OnError, StepAction, TaskGrant, TaskKind, TaskStatus, WatchSpec, WindowEvent,
};
use kivo_core::tool::{ConfirmSpec, ConfirmedBy, GrantDuration, Risk, Strength};
use kivo_core::{Event, EventMeta, ProfileId, SessionState, TaskId, Timestamp, TraceId, TurnId};
use kivo_ipc::infer::Residency;
use kivo_ipc::protocol::{
    ActivityItem, AgentItem, AgentSessionItem, AgentsOverview, AuditItem, BrainChip,
    CapabilityItem, Collision, ConnectorView, DesktopAiItem, DraftView, GrantItem, GrantLine,
    IslandPlacement, LiveActivity, McpFoundView, McpServerView, McpToolView, MeasuredItem,
    MemoryFolderView, MemoryNoteDetail, MemoryNoteView, MemoryOverview, MemorySuggestionView,
    MemoryTagView, ModelItem, Offer, ProfileItem, ProtocolVersion, QuietIsland, RecommendationItem,
    RoutineCheck, RoutineView, ScreenPoint, SetupAdvice, SkillView, SpeechChoices,
    SpeechEngineItem, SpeechStatus, StepView, TaskQuestion, TaskStepView, TaskView, ToolItem,
    TurnView, UndoOffer, VoiceItem, WorkspaceItem,
};
use kivo_ipc::{Link, LinkStatus, RpcError, StateSnapshot, Welcome, method};
use std::path::PathBuf;
use ts_rs::{Config, TS};

fn render() -> String {
    // JSON numbers, not bigint: every 64-bit integer KIVO sends fits in a double.
    let cfg = Config::new().with_large_int("number");
    let declarations = [
        // identifiers and time
        TurnId::decl(&cfg),
        TaskId::decl(&cfg),
        TraceId::decl(&cfg),
        ProfileId::decl(&cfg),
        Timestamp::decl(&cfg),
        // events
        Event::decl(&cfg),
        EventMeta::decl(&cfg),
        EventKind::decl(&cfg),
        VoiceEvent::decl(&cfg),
        TtsStopReason::decl(&cfg),
        TurnEvent::decl(&cfg),
        TurnSource::decl(&cfg),
        IntentPath::decl(&cfg),
        CancelReason::decl(&cfg),
        ToolEvent::decl(&cfg),
        PermissionDecision::decl(&cfg),
        TaskEvent::decl(&cfg),
        StepStatus::decl(&cfg),
        SystemEvent::decl(&cfg),
        DeviceKind::decl(&cfg),
        ProviderEvent::decl(&cfg),
        UiEvent::decl(&cfg),
        // session and protocol
        SessionState::decl(&cfg),
        PermissionMode::decl(&cfg),
        SoundCue::decl(&cfg),
        SoundSet::decl(&cfg),
        // what KIVO is doing now
        TurnView::decl(&cfg),
        BrainChip::decl(&cfg),
        UndoOffer::decl(&cfg),
        DraftView::decl(&cfg),
        LiveActivity::decl(&cfg),
        Offer::decl(&cfg),
        StepView::decl(&cfg),
        SpeechStatus::decl(&cfg),
        QuietIsland::decl(&cfg),
        ScreenPoint::decl(&cfg),
        Residency::decl(&cfg),
        // permissions and capabilities
        Risk::decl(&cfg),
        Strength::decl(&cfg),
        ConfirmedBy::decl(&cfg),
        ConfirmSpec::decl(&cfg),
        GrantDuration::decl(&cfg),
        Preset::decl(&cfg),
        Capability::decl(&cfg),
        Badge::decl(&cfg),
        StateSnapshot::decl(&cfg),
        IslandPlacement::decl(&cfg),
        IslandSpot::decl(&cfg),
        OverlayPosition::decl(&cfg),
        IslandSize::decl(&cfg),
        MotionPref::decl(&cfg),
        CompanionStyle::decl(&cfg),
        // Control Center lists
        ActivityItem::decl(&cfg),
        AuditItem::decl(&cfg),
        GrantItem::decl(&cfg),
        ModelItem::decl(&cfg),
        SpeechEngineItem::decl(&cfg),
        VoiceItem::decl(&cfg),
        MeasuredItem::decl(&cfg),
        ProfileItem::decl(&cfg),
        SpeechChoices::decl(&cfg),
        RecommendationItem::decl(&cfg),
        CapabilityItem::decl(&cfg),
        // tasks, routines, agents and workspaces (M5)
        TaskKind::decl(&cfg),
        TaskStatus::decl(&cfg),
        OnError::decl(&cfg),
        WindowEvent::decl(&cfg),
        WatchSpec::decl(&cfg),
        Criterion::decl(&cfg),
        StepAction::decl(&cfg),
        Notify::decl(&cfg),
        TaskGrant::decl(&cfg),
        TaskView::decl(&cfg),
        TaskStepView::decl(&cfg),
        TaskQuestion::decl(&cfg),
        Trigger::decl(&cfg),
        VarKind::decl(&cfg),
        VarDef::decl(&cfg),
        RoutineStep::decl(&cfg),
        Routine::decl(&cfg),
        RoutineView::decl(&cfg),
        GrantLine::decl(&cfg),
        Collision::decl(&cfg),
        RoutineCheck::decl(&cfg),
        ToolItem::decl(&cfg),
        WorkspaceItem::decl(&cfg),
        AgentsOverview::decl(&cfg),
        AgentItem::decl(&cfg),
        DesktopAiItem::decl(&cfg),
        AgentSessionItem::decl(&cfg),
        // MCP servers, connectors and skills (M6)
        McpServerView::decl(&cfg),
        McpToolView::decl(&cfg),
        McpFoundView::decl(&cfg),
        ConnectorView::decl(&cfg),
        SkillView::decl(&cfg),
        // the memory vault (M7)
        MemoryNoteView::decl(&cfg),
        MemoryTagView::decl(&cfg),
        MemoryFolderView::decl(&cfg),
        MemorySuggestionView::decl(&cfg),
        MemoryOverview::decl(&cfg),
        SetupAdvice::decl(&cfg),
        MemoryNoteDetail::decl(&cfg),
        ProtocolVersion::decl(&cfg),
        Welcome::decl(&cfg),
        RpcError::decl(&cfg),
        // the app's connection to the runtime
        LinkStatus::decl(&cfg),
        Link::decl(&cfg),
    ];
    let mut out = String::from(
        "// Generated from the Rust IPC types by crates/kivo-ipc/tests/ts_bindings.rs. Do not edit.\n\
         // Regenerate: KIVO_WRITE_TS=1 cargo test -p kivo-ipc --features ts --test ts_bindings\n",
    );
    for decl in declarations {
        out.push_str("\nexport ");
        out.push_str(&decl);
        out.push('\n');
    }
    // The requests the UI sends through the app (`runtime_request`).
    let methods = [
        ("sessionPause", method::SESSION_PAUSE),
        ("sessionResume", method::SESSION_RESUME),
        ("runtimeQuit", method::RUNTIME_QUIT),
        ("permissionsSetMode", method::PERMISSIONS_SET_MODE),
        ("sessionTalk", method::SESSION_TALK),
        ("sessionSay", method::SESSION_SAY),
        ("sessionCancel", method::SESSION_CANCEL),
        ("sessionStopEverything", method::SESSION_STOP_ALL),
        ("sessionUndo", method::SESSION_UNDO),
        ("permissionsAnswer", method::PERMISSIONS_ANSWER),
        ("permissionsGrants", method::PERMISSIONS_GRANTS),
        ("permissionsRevoke", method::PERMISSIONS_REVOKE),
        ("capabilitiesGet", method::CAPABILITIES_GET),
        ("capabilitiesSet", method::CAPABILITIES_SET),
        ("capabilitiesPreset", method::CAPABILITIES_PRESET),
        ("browserStatus", method::BROWSER_STATUS),
        ("activityList", method::ACTIVITY_LIST),
        ("auditList", method::AUDIT_LIST),
        ("modelsList", method::MODELS_LIST),
        ("modelsInstall", method::MODELS_INSTALL),
        ("modelsRemove", method::MODELS_REMOVE),
        ("modelsPause", method::MODELS_PAUSE),
        ("modelsCancel", method::MODELS_CANCEL),
        ("modelsSetDefault", method::MODELS_SET_DEFAULT),
        ("settingsGet", method::SETTINGS_GET),
        ("settingsSet", method::SETTINGS_SET),
        ("wakeList", method::WAKE_LIST),
        ("wakeCheck", method::WAKE_CHECK),
        ("wakeSave", method::WAKE_SAVE),
        ("wakeDelete", method::WAKE_DELETE),
        ("wakeSet", method::WAKE_SET),
        ("wakeHear", method::WAKE_HEAR),
        ("wakeTry", method::WAKE_TRY),
        ("wakeSample", method::WAKE_SAMPLE),
        ("wakeTune", method::WAKE_TUNE),
        ("wakeFalseAlarms", method::WAKE_FALSE_ALARMS),
        ("voiceIdStatus", method::VOICE_ID_STATUS),
        ("voiceIdStart", method::VOICE_ID_START),
        ("voiceIdRecord", method::VOICE_ID_RECORD),
        ("voiceIdFinish", method::VOICE_ID_FINISH),
        ("voiceIdCancel", method::VOICE_ID_CANCEL),
        ("voiceIdDelete", method::VOICE_ID_DELETE),
        ("soundsPreview", method::SOUNDS_PREVIEW),
        ("voiceEngines", method::VOICE_ENGINES),
        ("voiceRecommend", method::VOICE_RECOMMEND),
        ("voiceSwitch", method::VOICE_SWITCH),
        ("voicePreview", method::VOICE_PREVIEW),
        ("voiceMicCheck", method::VOICE_MIC_CHECK),
        ("voiceDevices", method::VOICE_DEVICES),
        ("voiceAdvanced", method::VOICE_ADVANCED),
        ("voiceTrySample", method::VOICE_TRY_SAMPLE),
        ("voiceVocabulary", method::VOICE_VOCABULARY),
        ("voiceAddWord", method::VOICE_ADD_WORD),
        ("voiceRemoveWord", method::VOICE_REMOVE_WORD),
        ("brainsCatalog", method::BRAINS_CATALOG),
        ("brainsList", method::BRAINS_LIST),
        ("brainsCheck", method::BRAINS_CHECK),
        ("brainsConnect", method::BRAINS_CONNECT),
        ("brainsDisconnect", method::BRAINS_DISCONNECT),
        ("brainsSetKey", method::BRAINS_SET_KEY),
        ("brainsTestKey", method::BRAINS_TEST_KEY),
        ("brainsSignIn", method::BRAINS_SIGN_IN),
        ("brainsSetDefault", method::BRAINS_SET_DEFAULT),
        ("brainsSaveProfile", method::BRAINS_SAVE_PROFILE),
        ("brainsDeleteProfile", method::BRAINS_DELETE_PROFILE),
        ("brainsDiscovery", method::BRAINS_DISCOVERY),
        ("brainsRefresh", method::BRAINS_REFRESH),
        ("brainsViewed", method::BRAINS_VIEWED),
        ("brainsSetWorkspace", method::BRAINS_SET_WORKSPACE),
        ("brainsContext", method::BRAINS_CONTEXT),
        ("usageSummary", method::USAGE_SUMMARY),
        ("usageSetLimits", method::USAGE_SET_LIMITS),
        ("usageSetCaps", method::USAGE_SET_CAPS),
        ("usageSetPrice", method::USAGE_SET_PRICE),
        ("usageExport", method::USAGE_EXPORT),
        ("chatThreads", method::CHAT_THREADS),
        ("chatThread", method::CHAT_THREAD),
        ("chatNew", method::CHAT_NEW),
        ("chatUpdate", method::CHAT_UPDATE),
        ("chatDelete", method::CHAT_DELETE),
        ("chatSend", method::CHAT_SEND),
        ("chatCompact", method::CHAT_COMPACT),
        ("chatContinue", method::CHAT_CONTINUE),
        ("chatBranch", method::CHAT_BRANCH),
        ("chatExport", method::CHAT_EXPORT),
        ("chatSearch", method::CHAT_SEARCH),
        ("chatMisroute", method::CHAT_MISROUTE),
        ("memoryPreferences", method::MEMORY_PREFERENCES),
        ("memorySetPreference", method::MEMORY_SET_PREFERENCE),
        ("memoryDeletePreference", method::MEMORY_DELETE_PREFERENCE),
        ("tasksList", method::TASKS_LIST),
        ("tasksGet", method::TASKS_GET),
        ("tasksCancel", method::TASKS_CANCEL),
        ("tasksPause", method::TASKS_PAUSE),
        ("tasksResume", method::TASKS_RESUME),
        ("tasksAnswer", method::TASKS_ANSWER),
        ("tasksDelete", method::TASKS_DELETE),
        ("tasksClear", method::TASKS_CLEAR),
        ("routinesList", method::ROUTINES_LIST),
        ("routinesSave", method::ROUTINES_SAVE),
        ("routinesDelete", method::ROUTINES_DELETE),
        ("routinesRun", method::ROUTINES_RUN),
        ("routinesEnable", method::ROUTINES_ENABLE),
        ("routinesCheck", method::ROUTINES_CHECK),
        ("routinesTools", method::ROUTINES_TOOLS),
        ("agentsOverview", method::AGENTS_OVERVIEW),
        ("agentsOpenInTerminal", method::AGENTS_OPEN_TERMINAL),
        ("agentsStart", method::AGENTS_START),
        ("workspacesList", method::WORKSPACES_LIST),
        ("workspacesRemember", method::WORKSPACES_REMEMBER),
        ("workspacesForget", method::WORKSPACES_FORGET),
        ("workspacesExportAgentsMd", method::WORKSPACES_EXPORT_AGENTS),
        ("instructionsGet", method::INSTRUCTIONS_GET),
        ("instructionsSet", method::INSTRUCTIONS_SET),
        ("permissionsBypass", method::PERMISSIONS_BYPASS),
        ("draftAnswer", method::DRAFT_ANSWER),
        ("sessionHelp", method::SESSION_HELP),
        ("sessionMissed", method::SESSION_MISSED),
        ("selectionAction", method::SELECTION_ACTION),
        ("offerAnswer", method::OFFER_ANSWER),
        ("mcpList", method::MCP_LIST),
        ("mcpFound", method::MCP_FOUND),
        ("mcpImport", method::MCP_IMPORT),
        ("mcpAdd", method::MCP_ADD),
        ("mcpRemove", method::MCP_REMOVE),
        ("mcpEnable", method::MCP_ENABLE),
        ("mcpApprove", method::MCP_APPROVE),
        ("mcpSetTool", method::MCP_SET_TOOL),
        ("mcpShare", method::MCP_SHARE),
        ("connectorsList", method::CONNECTORS_LIST),
        ("connectorsConnect", method::CONNECTORS_CONNECT),
        ("connectorsDisconnect", method::CONNECTORS_DISCONNECT),
        ("skillsList", method::SKILLS_LIST),
        ("skillsEnable", method::SKILLS_ENABLE),
        ("skillsImport", method::SKILLS_IMPORT),
        ("skillsRead", method::SKILLS_READ),
        ("skillsRemove", method::SKILLS_REMOVE),
        ("extensionsRefresh", method::EXTENSIONS_REFRESH),
        ("memoryOverview", method::MEMORY_OVERVIEW),
        ("memoryNote", method::MEMORY_NOTE),
        ("memorySave", method::MEMORY_SAVE),
        ("memoryRemember", method::MEMORY_REMEMBER),
        ("memoryRememberTurn", method::MEMORY_REMEMBER_TURN),
        ("memoryMeta", method::MEMORY_META),
        ("memoryDelete", method::MEMORY_DELETE),
        ("memoryForget", method::MEMORY_FORGET),
        ("memoryExport", method::MEMORY_EXPORT),
        ("memorySuggestion", method::MEMORY_SUGGESTION),
        ("memoryTidy", method::MEMORY_TIDY),
        ("memoryOpen", method::MEMORY_OPEN),
        ("memoryWhy", method::MEMORY_WHY),
        ("performanceStatus", method::PERFORMANCE_STATUS),
        ("setupRecommend", method::SETUP_RECOMMEND),
        ("diagnosticsRun", method::DIAGNOSTICS_RUN),
        ("settingsExport", method::SETTINGS_EXPORT),
        ("settingsImport", method::SETTINGS_IMPORT),
        ("settingsReset", method::SETTINGS_RESET),
        ("systemOpenUrl", method::SYSTEM_OPEN_URL),
    ];
    out.push_str("\n/** IPC methods the UI can call. */\nexport const Method = {\n");
    for (name, value) in methods {
        out.push_str(&format!("  {name}: \"{value}\",\n"));
    }
    out.push_str("} as const;\n\nexport type Method = (typeof Method)[keyof typeof Method];\n");
    out
}

#[test]
fn generated_typescript_is_up_to_date() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/kivo-app/src/ipc/generated.ts");
    let fresh = render();
    if std::env::var_os("KIVO_WRITE_TS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &fresh).unwrap();
        return;
    }
    let current = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        current == fresh,
        "apps/kivo-app/src/ipc/generated.ts is stale; regenerate with:\n  \
         KIVO_WRITE_TS=1 cargo test -p kivo-ipc --features ts --test ts_bindings"
    );
}
