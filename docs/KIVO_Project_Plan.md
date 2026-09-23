# K.I.V.O. — Master Product & Engineering Blueprint

> **K.I.V.O. = Knowledge • Intelligence • Voice • Operations**
>
> **Product:** KIVO  
> **Category:** Windows-first personal AI desktop operating companion  
> **Primary interface:** Voice  
> **Core philosophy:** Local speed + interchangeable intelligence + native computer control + strong user control  
> **Document purpose:** Complete product, UX, architecture, engineering, security, performance, testing, release, and long-term expansion blueprint.
>
> **Implementation rule:** This document intentionally contains architecture and implementation instructions, but no production source code. The document defines *what to build, why to build it, how the parts interact, and how the finished system should behave.*
>
> **Companion documents (added 2026-09-21):** engineering specs in [architecture/](architecture/ARCHITECTURE.md), milestones in [ROADMAP.md](ROADMAP.md), decisions in [DECISIONS.md](DECISIONS.md), research in [research/](research/). Where they refine or change this blueprint, §167 lists the amendments. §168 maps every section to the build-checklist items that implement it; how items are tracked is in [README.md](README.md).

---

# 1. Executive Summary

KIVO is a Windows-first personal AI desktop companion designed to make interacting with a computer feel as natural as talking to a highly capable assistant.

KIVO is not intended to be merely:

- a chatbot
- a voice-to-text application
- a launcher
- a screen-control macro
- an AI agent wrapper
- an MCP client
- a local LLM frontend

Instead, KIVO is an **AI orchestration and computer-operation runtime**.

It combines:

1. a lightweight always-available local runtime,
2. local wake-word and voice infrastructure,
3. configurable speech recognition,
4. configurable speech synthesis,
5. multiple interchangeable AI/agent backends,
6. a deterministic fast-command system,
7. a capability-aware computer-control layer,
8. Windows-native automation,
9. browser automation,
10. MCP interoperability,
11. background/event-driven tasks,
12. permissions and security policies,
13. memory and task state,
14. a premium desktop control center,
15. an optional animated companion,
16. adaptive hardware/resource management,
17. benchmarking and self-diagnostics.

The user should not need to understand any of the underlying systems.

They should be able to say:

> “Hey Kivo, open my project.”

or:

> “Kivo, fix the failing tests.”

or:

> “Kivo, watch this download and tell me when it's done.”

and KIVO should determine the most appropriate mechanism.

---

# 2. Product Name and Branding

## 2.1 Official Name

# K.I.V.O.

Expanded as:

# Knowledge • Intelligence • Voice • Operations

The application can display the short form:

# KIVO

The dotted form is reserved primarily for branding, logo treatments, marketing, and formal documentation.

---

## 2.2 Meaning of the Name

### Knowledge

KIVO understands and works with:

- files
- applications
- documents
- browser context
- projects
- system state
- user-provided information
- task context
- permitted long-term preferences

### Intelligence

KIVO can connect to:

- cloud APIs
- AI aggregators
- external CLI agents
- local models
- coding agents
- specialized AI services
- future providers

### Voice

Voice is the primary interaction layer:

- wake word
- speech recognition
- speech synthesis
- interruption
- conversational turn-taking
- voice commands
- optional push-to-talk

### Operations

KIVO can actually operate the computer:

- applications
- windows
- files
- browser
- terminal
- UI controls
- workflows
- background tasks
- system functions

---

# 3. Product Philosophy

KIVO follows five foundational principles.

## Principle 1 — AI is not the operating system

KIVO itself is the operating layer.

AI is a replaceable intelligence component.

This distinction prevents the project from becoming dependent on one model vendor.

---

## Principle 2 — Use the cheapest mechanism that safely solves the problem

For:

> “Mute.”

Use a native operation.

For:

> “Open Chrome.”

Use the Windows/application launch mechanism.

For:

> “Fix my broken project.”

Use an AI agent.

For:

> “Summarize this private document without uploading it.”

Use an approved local model.

---

## Principle 3 — Semantic control before visual control

Prefer:

1. native API
2. application API
3. OS command/API
4. UI Automation
5. browser DOM/accessibility
6. vision
7. mouse/keyboard simulation

This improves:

- reliability
- speed
- accessibility
- explainability
- resource usage

---

## Principle 4 — Everything should be interruptible

The user must always be able to say:

> “Stop.”

KIVO should prioritize interruption above normal execution.

---

## Principle 5 — The computer should remain understandable to the user

KIVO should not become an opaque autonomous system.

The user should be able to see:

- what KIVO is doing
- which brain is being used
- which tool is executing
- what permissions are involved
- what failed
- how to stop it

---

# 4. Product Goals

## Primary goals

KIVO should:

- wake quickly
- understand speech quickly
- respond naturally
- consume little idle hardware
- avoid unnecessary AI calls
- use the best available local/native capability
- support many AI providers
- support multiple providers simultaneously
- work offline for appropriate functions
- operate Windows reliably
- support complex agentic tasks
- be visually premium
- remain extensible
- remain secure
- remain understandable

---

# 5. Non-Goals

KIVO should not initially attempt to:

- replace the Windows shell completely
- replace every application
- force users into one AI provider
- require a local GPU
- require an expensive subscription
- constantly observe the screen
- constantly send data to cloud AI
- autonomously perform high-risk actions without appropriate authorization
- depend entirely on MCP
- depend entirely on a single external agent
- require a heavyweight 3D avatar

---

# 6. Target User Experience

The ideal experience is:

```text
User:
"Hey Kivo, open my work project."

KIVO:
[Immediately wakes]
[Streams speech recognition]
[Classifies request]
[Launches application/project]
[Provides short response]

User:
"Kivo, now check why the tests are failing."

KIVO:
[Recognizes coding task]
[Selects configured coding brain]
[Starts agent]
[Shows activity]
[Executes tools]
[Runs validation]
[Reports result]
```

The user should not need to manually switch applications or models.

---

# 7. High-Level System Architecture

```text
                               ┌─────────────────────┐
                               │        USER         │
                               │ Voice / UI / Input  │
                               └──────────┬──────────┘
                                          │
                                          ▼
                               ┌─────────────────────┐
                               │    KIVO RUNTIME     │
                               │      Rust Core      │
                               └──────────┬──────────┘
                                          │
                    ┌─────────────────────┼─────────────────────┐
                    │                     │                     │
                    ▼                     ▼                     ▼
             ┌────────────┐       ┌──────────────┐      ┌──────────────┐
             │ VOICE      │       │ EVENT BUS    │      │ CONTROL UI   │
             │ PIPELINE   │       │              │      │ / COMPANION  │
             └─────┬──────┘       └──────┬───────┘      └──────────────┘
                   │                     │
                   ▼                     │
             ┌────────────┐              │
             │    VAD     │              │
             └─────┬──────┘              │
                   ▼                     │
             ┌────────────┐              │
             │ WAKE WORD  │              │
             └─────┬──────┘              │
                   ▼                     │
             ┌────────────┐              │
             │    STT     │              │
             └─────┬──────┘              │
                   └──────────┬──────────┘
                              ▼
                     ┌────────────────┐
                     │ INTENT ROUTER  │
                     └───────┬────────┘
                             │
             ┌───────────────┼─────────────────┐
             │               │                 │
             ▼               ▼                 ▼
       ┌──────────┐   ┌──────────────┐  ┌──────────────┐
       │ FAST     │   │ BRAIN ROUTER │  │ EVENT TASK   │
       │ PATH     │   └──────┬───────┘  │ PATH         │
       └────┬─────┘          │          └──────┬───────┘
            │                │                 │
            │       ┌────────┼────────┐        │
            │       │        │        │        │
            │       ▼        ▼        ▼        │
            │      API      CLI     LOCAL      │
            │     BRAIN    AGENT    BRAIN      │
            │       │        │        │        │
            └───────┴────────┼────────┴────────┘
                             ▼
                      ┌──────────────┐
                      │ TOOL ROUTER  │
                      └──────┬───────┘
                             │
          ┌──────────────────┼──────────────────┐
          │                  │                  │
          ▼                  ▼                  ▼
      Native/API            UIA             Browser
          │                  │                  │
          └──────────────────┼──────────────────┘
                             │
                    ┌────────┼─────────┐
                    │                  │
                    ▼                  ▼
                  Vision        Mouse/Keyboard
                    │                  │
                    └────────┬─────────┘
                             ▼
                          WINDOWS
                             │
                             ▼
                          RESULT
                             │
                ┌────────────┼────────────┐
                ▼            ▼            ▼
               TTS       Companion    Control UI
```

---

# 8. Process Architecture

KIVO should be separated into independently restartable conceptual components.

## 8.1 KIVO Runtime

Always-running core.

Responsibilities:

- lifecycle
- configuration
- event bus
- state
- audio coordination
- wake handling
- routing
- tools
- permissions
- task orchestration
- provider management
- logging

---

## 8.2 Voice Worker

Responsible for:

- audio capture
- VAD
- wake word
- STT
- TTS coordination

Voice processing should be isolated enough that a provider failure does not crash the main runtime.

---

## 8.3 Brain Worker/Adapter

Each provider should be isolated behind a common interface.

A broken provider should not bring down KIVO.

---

## 8.4 Tool Worker

Tools should have:

- timeouts
- cancellation
- structured results
- permissions
- resource limits

---

## 8.5 Control Center

Tauri desktop application.

It can connect to the runtime through a local IPC mechanism.

The Control Center can restart without stopping KIVO.

---

## 8.6 Companion

Optional transparent window.

The companion should not contain core business logic.

---

# 9. Core Technology Strategy

## Runtime

Preferred:

- Rust
- asynchronous runtime
- Windows APIs
- SQLite
- local IPC
- structured configuration

## UI

Preferred:

- Tauri v2
- modern web UI
- WebView2
- hardware-accelerated rendering where appropriate

## Voice

Model/provider abstraction.

## AI

Provider abstraction.

## Computer control

Native Windows + UI Automation + browser automation + fallbacks.

---

# 10. Why Rust

Rust is preferred for:

- low idle overhead
- predictable memory behavior
- fast startup
- native Windows access
- concurrency
- process control
- IPC
- safety
- long-running stability

KIVO should avoid making Python the foundation of its resident runtime.

Python can be used in isolated environments when an ML engine requires it.

---

# 11. AI Brain Architecture

The Brain system is one of the most important parts of KIVO.

It must be provider-neutral.

Conceptually:

```text
                 KIVO BRAIN
                     │
       ┌─────────────┼─────────────┐
       │             │             │
      API           CLI          LOCAL
       │             │             │
 Cloud providers  Agents      Local runtimes
```

---

# 12. Brain Provider Types

## Type A — API

Examples:

- OpenAI
- Anthropic
- Google
- OpenRouter
- Groq
- Together
- Mistral
- xAI
- DeepSeek
- Cerebras
- other providers
- custom endpoints

The supported provider catalog should be extensible.

---

## Type B — CLI

Examples may include:

- Claude Code
- Codex
- Gemini CLI
- future coding agents
- custom agent CLI

KIVO should treat these as external agent runtimes.

---

## Type C — Local

Examples:

- Ollama
- LM Studio
- llama.cpp-based servers
- OpenAI-compatible local endpoints
- custom local inference servers

---

## Type D — Managed Login

Support providers that authenticate through:

- OAuth
- browser login
- subscription account
- provider CLI authentication

Do not assume every future AI provider uses API keys.

---

# 13. Provider Adapter Contract

Every provider adapter should conceptually provide:

- provider identity
- authentication
- available models
- model capabilities
- streaming
- tool support
- cancellation
- health checking
- error normalization
- context capability
- cost metadata where available
- privacy classification
- availability state

---

# 14. Multiple Provider Configuration

A user can configure many brains.

Example:

```text
Default:
Anthropic

Fast:
OpenAI

Coding:
Codex

Local:
Ollama

Fallback:
OpenRouter

Private:
Local model
```

KIVO should not require one global provider.

---

# 15. Brain Profiles

Built-in profiles:

- Default
- Fast
- Smart
- Coding
- Private
- Offline
- Cheap
- Custom

Users can create additional profiles.

---

# 16. Brain Routing Rules

The Brain Router should consider:

- request type
- user-selected profile
- provider capability
- latency
- availability
- privacy
- local/online state
- cost preference
- context requirements
- tool requirements
- current machine load

---

# 17. API Configuration UX

The user should see:

```text
Add Brain

○ Cloud/API
○ CLI/Agent
○ Local
○ Account Login
○ Custom Endpoint
```

For Cloud/API:

```text
Provider
API Key
Default Model
Streaming
Privacy
Test Connection
```

The application should validate credentials immediately.

---

# 18. CLI Discovery

KIVO should scan for known supported CLI agents.

Detection should identify:

- executable path
- version
- installation status
- authentication state where detectable
- supported arguments
- streaming support
- structured output
- headless mode
- tool support

The result should be shown clearly.

---

# 19. CLI Installation

KIVO should not blindly install every dependency.

Instead:

```text
CLI selected
 ↓
Dependency check
 ↓
Missing dependency
 ↓
Explain requirement
 ↓
Install with consent
 ↓
Verify
 ↓
Authenticate
 ↓
Test
```

---

# 20. Dependency Manager

Potential dependencies:

- Node.js
- Python
- Git
- FFmpeg
- Playwright
- Ollama
- other provider runtimes

Dependencies should be:

- version checked
- health checked
- upgradeable
- removable where practical
- isolated where possible

---

# 21. Base Installer Philosophy

The MSI should contain the essentials.

Do not make the base installer enormous by bundling every model and every external runtime.

Base:

- KIVO Runtime
- Control Center
- core assets
- configuration
- voice infrastructure
- updater
- diagnostic system

Optional:

- AI models
- CLI agents
- local runtimes
- additional voices
- browser dependencies

---

# 22. Local AI

KIVO should support local AI as a first-class option.

Reasons:

- privacy
- offline use
- predictable cost
- fallback
- specialized local workflows

However, KIVO should not require users to run a local LLM.

---

# 23. STT Architecture

STT is provider-based.

Potential engines:

- Parakeet
- Silero
- Whisper-family engines
- cloud speech services
- future optimized engines

No engine should be architecturally mandatory.

---

# 24. STT Selection UX

Each model should display:

- speed
- accuracy
- CPU usage
- GPU requirement
- RAM/VRAM
- offline status
- supported languages
- recommendation

Example:

```text
Ultra Fast
★★★★★ speed
★★★ accuracy
Very low resources

Balanced
★★★★ speed
★★★★ accuracy
Low resources

High Accuracy
★★★ speed
★★★★★ accuracy
Higher resources

Cloud
Provider dependent
Internet required
```

---

# 25. STT Hardware Detection

KIVO should detect:

- CPU
- RAM
- GPU
- VRAM
- NPU where supported
- battery state
- current system load

Then recommend a voice configuration.

---

# 26. Adaptive STT Residency

States:

```text
DEEP IDLE
   ↓
WAKE
   ↓
WARMING
   ↓
ACTIVE
   ↓
WARM
   ↓
IDLE
   ↓
DEEP IDLE
```

Recent interaction can keep STT warm.

Long inactivity can unload it.

---

# 27. Wake Word

The wake-word subsystem should be tiny compared with the main reasoning models.

Pipeline:

```text
Microphone
 ↓
VAD
 ↓
Wake Detector
 ↓
Activate STT
```

The wake layer must run continuously with minimal resource usage.

---

# 28. Push-to-Talk

Push-to-talk is mandatory as a fallback.

Benefits:

- noisy rooms
- privacy
- accessibility
- predictable activation
- testing

---

# 29. VAD

VAD should identify:

- speech start
- speech continuation
- speech end
- interruption
- barge-in

VAD should preferably remain local.

---

# 30. Endpointing

Avoid excessive silence waits.

Endpointing should use:

- silence duration
- transcript stability
- linguistic boundaries
- semantic completion
- user speech pattern

---

# 31. Streaming STT

The STT layer should expose:

- partial text
- stable text
- final text
- timestamps if available
- confidence where available

---

# 32. TTS Architecture

TTS should be provider-based.

Potential options:

- Orca
- Kokoro
- system TTS
- other local engines
- cloud TTS

---

# 33. TTS Selection UX

Display:

- latency
- naturalness
- resource usage
- offline capability
- voice selection
- expressiveness

---

# 34. Streaming TTS

Pipeline:

```text
LLM stream
 ↓
Phrase/sentence segmentation
 ↓
TTS stream
 ↓
Audio playback
```

The system should start speaking before the complete response is generated when appropriate.

---

# 35. Barge-In

When KIVO speaks and the user starts speaking:

1. detect voice
2. stop or fade TTS
3. cancel obsolete generation
4. cancel unnecessary tools
5. begin new STT turn
6. execute new request

---

# 36. Voice Personality

The voice should be:

- clear
- concise
- natural
- calm
- responsive

Avoid excessive verbal filler.

The companion can provide visual acknowledgement instead of forcing spoken filler.

---

# 37. Fast Intent Router

This is a major performance component.

It should identify whether a request needs a brain.

Examples:

```text
Mute
→ native

Open Chrome
→ native

Take screenshot
→ native

What is this document?
→ brain

Fix this code
→ coding agent

Tell me when the download finishes
→ event task
```

---

# 38. Deterministic Fast Path

The fast path should support:

## System

- volume
- mute
- microphone
- brightness where supported
- lock
- sleep
- restart
- shutdown with confirmation

## Windows

- minimize
- maximize
- focus
- switch
- close
- screenshot

## Applications

- launch
- close
- focus
- restart

## Files

- search
- open
- rename
- move
- create

## Media

- play
- pause
- next
- previous

---

# 39. Agent Path

Complex requests:

```text
Request
 ↓
Intent
 ↓
Brain selection
 ↓
Planning
 ↓
Tool selection
 ↓
Execution
 ↓
Validation
 ↓
Response
```

---

# 40. Planner

Planner determines:

- objective
- constraints
- dependencies
- subtasks
- parallel work
- required tools
- confirmation points
- success criteria

Do not use complex planning for simple tasks.

---

# 41. Task Graph

Complex work should be represented as a graph.

Example:

```text
Inspect project
      ↓
Run tests
      ↓
Analyze failure
      ↓
Modify code
      ↓
Run tests
      ↓
Validate
```

Independent operations can execute concurrently.

---

# 42. Validation

KIVO should not blindly trust tool output.

For important tasks:

1. perform action
2. verify result
3. only then report success

Example:

> “I fixed it.”

should mean the appropriate validation succeeded.

---

# 43. Cancellation

Cancellation must propagate through all layers.

Possible cancellation sources:

- user voice
- UI button
- global shortcut
- task timeout
- provider failure
- application shutdown

---

# 44. Tool Router

The Tool Router maps intent to the most appropriate execution mechanism.

It should know:

- capabilities
- permissions
- risk
- latency
- availability
- side effects

---

# 45. Capability Ladder

Preferred order:

```text
1. Native API
2. OS API
3. App CLI
4. UI Automation
5. Browser DOM
6. Accessibility tree
7. Vision
8. Mouse/keyboard
```

The router can override this when a specific method is known to be more reliable.

---

# 46. Native Windows Tools

Core capabilities:

- process management
- windows
- filesystem
- clipboard
- audio
- notifications
- system state
- application launching

---

# 47. Windows UI Automation

UI Automation should be a foundational capability.

It provides semantic access to:

- controls
- windows
- properties
- patterns
- states
- actions
- bounds
- events

KIVO should use UI Automation before coordinate-based clicking whenever available.

---

# 48. UIA Event Integration

Use events for:

- focus changes
- property changes
- window changes
- element changes
- state changes

This minimizes polling.

---

# 49. Browser Control

Priority:

1. browser API
2. DOM
3. accessibility information
4. browser automation
5. vision
6. raw input

Browser context should be structured.

---

# 50. Browser Context

Possible context:

- browser
- window
- tab
- URL
- title
- selected text
- relevant DOM
- page state

Do not send entire pages to the brain unnecessarily.

---

# 51. Vision

Vision is a fallback.

Use it when:

- semantic controls are unavailable
- application is visually rendered
- UIA is insufficient
- visual interpretation is needed

Avoid continuous screenshots.

---

# 52. Mouse/Keyboard

Last-resort control.

Capabilities:

- click
- double click
- right click
- drag
- type
- shortcuts
- scroll

Every action must remain subject to permission policy.

---

# 53. MCP

MCP is an interoperability layer.

Use it for:

- third-party integrations
- reusable external tools
- external agent compatibility
- plugin ecosystems
- standardized integrations

Do not route every internal operation through MCP.

---

# 54. MCP Manager

The Control Center should show:

- server list
- status
- permissions
- tools
- configuration
- logs
- health

---

# 55. Dynamic MCP Tool Exposure

Only expose relevant tools to the active brain.

This reduces:

- context size
- token usage
- confusion
- latency
- accidental tool selection

---

# 56. Tool Risk Classification

Every tool should have a risk level.

### Safe

Read-only information.

### Low

Non-destructive local operations.

### Medium

Modifications.

### High

Destructive, financial, external communication, or security-sensitive actions.

---

# 57. Permission Engine

Architecture:

```text
AI
 ↓
Tool Request
 ↓
Permission Engine
 ↓
Policy Evaluation
 ↓
Confirmation if required
 ↓
Tool Execution
```

The AI cannot bypass the permission layer.

---

# 58. Permission Profiles

Possible profiles:

- Maximum Safety
- Balanced
- Power User
- Custom

Users can override specific tools.

---

# 59. Shell Access

Support:

- PowerShell
- Command Prompt
- Windows Terminal
- application CLIs

Shell execution must have:

- timeout
- working directory
- output capture
- cancellation
- permissions
- logging
- risk classification

---

# 60. Prompt Injection Defense

Treat content from:

- websites
- documents
- emails
- files
- clipboard

as untrusted data.

External text cannot grant itself permission.

---

# 61. Credential Security

Credentials should never be inserted into normal AI prompts unless absolutely unavoidable.

Preferred flow:

```text
Brain
 ↓
Tool
 ↓
Secure credential store
 ↓
Service
```

---

# 62. Memory Architecture

Separate:

### Conversation history

What was said.

### Task state

What is currently being done.

### Preferences

What the user explicitly wants.

### Long-term memory

Information deliberately retained.

### Runtime state

Temporary machine information.

---

# 63. Memory Rules

Do not automatically turn every conversation into permanent memory.

Memory must be:

- explainable
- inspectable
- removable
- privacy-aware
- scoped

---

# 64. Context Management

Use three context levels.

### Always available

- current task
- current app
- user preferences
- permission state

### On demand

- browser state
- files
- UI tree
- clipboard

### Task-specific

- project information
- selected documents
- relevant history

---

# 65. Context Deltas

Instead of repeatedly sending full state, send changes.

Example:

```text
Previous:
Active app = Chrome

Delta:
Active app changed to VS Code
Focused file = main.rs
```

---

# 66. Event Bus

Central event categories:

```text
WakeDetected
SpeechStarted
TranscriptUpdated
SpeechEnded
IntentDetected
BrainSelected
TaskStarted
ToolStarted
ToolCompleted
ToolFailed
WindowChanged
FileChanged
BrowserChanged
TTSStarted
TTSStopped
TaskCancelled
TaskCompleted
```

---

# 67. Background Automation

KIVO should support tasks that continue without an active conversation.

Examples:

- download watcher
- folder watcher
- build watcher
- process watcher
- calendar/reminder events
- application state watcher

Prefer native events over polling.

---

# 68. Scheduler

A future scheduler can support:

- one-time tasks
- recurring tasks
- conditional tasks
- event-triggered tasks

Every scheduled task should have:

- owner
- permissions
- execution history
- cancellation
- status

---

# 69. Privacy Modes

## Cloud

Normal provider behavior.

## Local

Prefer local processing.

## Strict Private

Block cloud transfer unless explicitly approved.

## Custom

Rules per provider/tool/data type.

---

# 70. Data Classification

Classify:

- public
- normal
- personal
- sensitive
- credential
- highly sensitive

Provider routing must respect classification.

---

# 71. Offline Mode

Offline operation should support:

- wake
- STT
- TTS
- native commands
- files
- UIA
- local tools
- local brain
- local automation

---

# 72. Network Resilience

Handle:

- offline
- timeout
- provider failure
- rate limit
- authentication failure
- degraded connection

Provide useful fallback behavior.

---

# 73. Failover

Provider failure:

```text
Primary
 ↓
Failure
 ↓
Check policy
 ↓
Fallback
 ↓
Continue
```

Privacy rules always override convenience.

---

# 74. Companion

The companion is optional.

Modes:

- hidden
- ambient
- compact
- full
- presentation

---

# 75. Companion State Machine

```text
IDLE
 ↓
LISTENING
 ↓
THINKING
 ↓
ACTING
 ↓
SPEAKING
 ↓
IDLE
```

Other states:

- ERROR
- WAITING
- CONFIRMATION
- INTERRUPTED
- PAUSED

---

# 76. Companion Visual Behavior

Idle:

- minimal animation

Listening:

- subtle active response

Thinking:

- restrained processing animation

Acting:

- movement corresponding to action

Speaking:

- voice-synchronized movement

Pointing:

- target-directed animation

---

# 77. Companion Targeting

Preferred:

```text
Semantic target
 ↓
UIA/DOM target
 ↓
Bounds
 ↓
Screen coordinates
 ↓
Companion pointer
```

Vision is fallback.

---

# 78. Companion Rendering

Prefer:

- SVG
- WebP
- sprite sheets
- lightweight animations
- Lottie where useful

Avoid heavy 3D engines initially.

---

# 79. UI/UX Philosophy

KIVO should have a premium, modern, Mac-like interaction quality while remaining unmistakably a Windows application.

Characteristics:

- clean
- elegant
- dark-first
- optional light mode
- rounded surfaces
- subtle translucency
- excellent typography
- smooth transitions
- restrained animation
- strong hierarchy
- minimal clutter

Avoid generic “AI dashboard” design.

---

# 80. Control Center Information Architecture

```text
KIVO

Home
Chat
Activity
Tasks

Brains
Voice
Tools
MCP
Integrations

Memory
Permissions
Privacy

Companion
Performance

Settings
Diagnostics
About
```

---

# 81. Home

Show:

- KIVO status
- active brain
- voice status
- current task
- recent activity
- quick actions

Do not turn Home into an overwhelming analytics screen.

---

# 82. Chat

Chat should support:

- text
- voice
- attachments
- task state
- tool activity
- cancellation
- model/provider visibility
- result summaries

---

# 83. Activity

Timeline of important actions.

Example:

```text
10:42 Wake detected
10:42 Request recognized
10:42 Fast command selected
10:42 Chrome launched
10:42 Completed
```

---

# 84. Tasks

Long-running tasks should show:

- name
- status
- current operation
- elapsed time
- resources
- tool activity
- cancellation
- result

---

# 85. Brain Settings

Users should be able to:

- add provider
- remove provider
- test provider
- select model
- create profiles
- configure fallback
- configure privacy
- configure cost/latency preference

---

# 86. Voice Settings

Show:

- microphone
- speaker
- wake word
- VAD
- STT
- TTS
- voice
- latency
- resource usage

---

# 87. Tool Settings

Display:

- tool
- source
- capability
- risk
- permission
- status

---

# 88. MCP Settings

Display:

- server
- status
- tools
- permissions
- configuration
- health
- logs

---

# 89. Performance Dashboard

Show:

- CPU
- RAM
- GPU
- VRAM
- NPU if available
- model residency
- STT latency
- brain latency
- tool latency
- TTS latency
- total latency

---

# 90. Hardware-Aware Configuration

KIVO should create a hardware profile.

Possible profiles:

- Low Resource
- Balanced
- Performance
- Battery
- Gaming
- Custom

---

# 91. GPU Policy

Consider:

- current GPU load
- VRAM
- active applications
- battery
- gaming
- model requirements

KIVO should avoid aggressively consuming GPU resources while a user is performing GPU-heavy work.

---

# 92. NPU Support

Architect for CPU/GPU/NPU/cloud as interchangeable compute backends.

Future voice and AI components can use NPU acceleration where supported.

---

# 93. Adaptive Model Residency

Models should be able to transition:

```text
UNLOADED
 ↓
WARMING
 ↓
WARM
 ↓
ACTIVE
 ↓
IDLE
 ↓
UNLOADING
```

---

# 94. Predictive Prewarming

After wake detection:

- begin preparing STT
- prepare likely tool subsystem
- optionally warm the likely provider

Never perform side effects merely because a request seems likely.

---

# 95. Fast Acknowledgement

For long tasks, provide immediate feedback using:

- companion
- UI
- short local phrase where appropriate

Do not create unnecessary speech filler.

---

# 96. Performance Philosophy

Optimize perceived latency first.

The critical chain is:

```text
Wake
 ↓
Speech
 ↓
STT
 ↓
Intent
 ↓
Brain/tool
 ↓
TTS
```

Measure every segment.

---

# 97. Latency Instrumentation

Record:

```text
T0 Wake
T1 STT activation
T2 First partial transcript
T3 Final transcript
T4 Request dispatch
T5 First brain token
T6 First tool decision
T7 First tool execution
T8 First TTS audio
T9 First audible response
T10 Task completion
```

---

# 98. Benchmark Harness

Benchmark:

- cold start
- warm start
- first token
- first audio
- first tool
- total completion
- CPU
- RAM
- GPU
- VRAM
- battery impact

---

# 99. STT Benchmarking

Compare:

- startup
- warm inference
- transcription speed
- accuracy
- CPU
- GPU
- memory
- latency

Do not select defaults solely from marketing claims.

---

# 100. TTS Benchmarking

Measure:

- first-audio latency
- synthesis speed
- CPU
- GPU
- memory
- perceived quality
- interruption latency

---

# 101. Brain Benchmarking

Measure:

- time to first token
- streaming stability
- tool-call latency
- structured output reliability
- cancellation
- error rate
- cost where applicable

---

# 102. End-to-End Benchmarking

Measure real user journeys:

### Simple

“Mute.”

### Medium

“Open Chrome and search for X.”

### Complex

“Inspect my project, find the test failure, fix it, and verify.”

---

# 103. Performance Targets

Targets should be treated as engineering goals, not guarantees.

Desired experience:

- wake feels immediate
- simple actions feel nearly instantaneous
- first tool action begins quickly
- speech begins as early as practical
- interruption is immediate
- idle resource usage is low

Actual thresholds should be established empirically.

---

# 104. Security Architecture

KIVO has high privilege and therefore requires a defense-in-depth architecture.

Layers:

1. OS security
2. process isolation
3. permission engine
4. tool risk classification
5. credential isolation
6. prompt-injection defenses
7. confirmation UX
8. audit logging
9. emergency stop

---

# 105. Emergency Stop

Provide:

- global shortcut
- tray button
- Control Center button
- voice command

Emergency stop should immediately stop:

- TTS
- pending generation
- noncritical tools
- automation
- background agent activity where possible

---

# 106. Audit Log

Important actions record:

- timestamp
- task
- tool
- action
- permission
- confirmation
- result
- failure

Secrets are excluded.

---

# 107. Configuration

Configuration should be:

- versioned
- validated
- migratable
- backed up
- human-readable where practical

Categories:

- general
- brains
- voice
- tools
- privacy
- memory
- companion
- performance
- integrations
- permissions
- automation

---

# 108. Database

SQLite should store:

- task history
- event metadata
- preferences
- provider metadata
- automation state
- benchmark results
- memory metadata
- configuration metadata

Secrets should be stored separately.

---

# 109. Diagnostics

Diagnostic bundle may contain:

- KIVO version
- Windows version
- hardware
- enabled modules
- provider health
- performance metrics
- recent errors

It must exclude:

- API keys
- passwords
- credentials
- sensitive document contents

---

# 110. Installer

The MSI should:

- install runtime
- install Control Center
- register startup option
- install required components
- create shortcuts where selected
- register uninstaller
- support upgrades

---

# 111. Dependency Installer

When a dependency is required:

```text
Detect
 ↓
Explain
 ↓
Consent
 ↓
Install
 ↓
Verify
 ↓
Configure
 ↓
Test
```

---

# 112. Update System

Updates should support:

- application
- runtime
- UI
- adapters
- assets
- optional models

Use safe update/rollback behavior.

---

# 113. Release Channels

Support:

- Stable
- Beta
- Experimental

Users should be able to select their channel.

---

# 114. Crash Recovery

If a worker crashes:

- isolate the failure
- restart it where safe
- preserve task state
- report the problem
- avoid cascading failure

---

# 115. Testing Strategy

## Unit tests

- routing
- permissions
- state
- configuration
- provider adapters
- task graph

## Integration tests

- Windows APIs
- UIA
- browser
- MCP
- voice
- CLI agents

## End-to-end

- voice to command
- voice to agent
- task cancellation
- provider fallback
- offline mode
- privacy mode

---

# 116. Safe Computer-Control Test Environment

Build a controlled environment containing:

- dummy applications
- dummy browser pages
- synthetic files
- safe terminal
- predictable UI

Never make the real user's desktop the only test environment.

---

# 117. Security Testing

Test against:

- prompt injection
- malicious webpage instructions
- malicious documents
- hostile MCP servers
- shell injection
- credential leakage
- path traversal
- permission bypass
- unsafe model output
- destructive tool misuse

---

# 118. Accessibility

Support:

- keyboard navigation
- focus indicators
- screen readers where practical
- reduced motion
- high contrast
- scalable text
- voice-only operation

---

# 119. Internationalization

Architecture should support:

- translated UI
- multiple speech languages
- multilingual TTS
- provider language capabilities
- localized date/time formats

English can be the initial primary language.

---

# 120. Application Capability Registry

Each application integration can declare:

- identity
- launch method
- API
- CLI
- UIA support
- browser support
- special capabilities

Example:

```text
VS Code
 ├── Launch
 ├── Workspace
 ├── Files
 ├── Terminal
 └── CLI

Chrome
 ├── Launch
 ├── Tabs
 ├── Navigation
 ├── DOM
 └── Browser automation
```

---

# 121. Plugin Architecture

Future plugins may add:

- providers
- tools
- voices
- integrations
- companion assets
- UI panels
- commands

Plugins must have permissions.

---

# 122. Tool Schema Design

Every tool should expose:

- identity
- purpose
- parameters
- result
- risk
- permission
- timeout
- cancellation
- side effects

Tool schemas should be concise.

---

# 123. Dynamic Tool Selection

Before a model receives tools:

1. classify task
2. identify capabilities
3. retrieve relevant tools
4. expose minimal tool set

This reduces context overhead.

---

# 124. State Synchronization

The Runtime is authoritative.

Control Center and Companion are clients.

```text
Runtime
  │
  ├── Control Center
  └── Companion
```

Neither UI should independently maintain authoritative task state.

---

# 125. IPC

Communication between runtime and UI should be:

- local
- authenticated/validated
- structured
- versioned
- event-capable
- reconnectable

---

# 126. Startup Sequence

Ideal:

```text
Windows starts
 ↓
KIVO Runtime starts
 ↓
Load lightweight configuration
 ↓
Initialize event bus
 ↓
Initialize audio
 ↓
Initialize wake/VAD
 ↓
Register OS/application events
 ↓
Remain lightweight
```

Heavy models are loaded according to policy.

---

# 127. Shutdown Sequence

On shutdown:

1. stop new tasks
2. notify active tasks
3. cancel cancellable work
4. stop audio
5. stop providers
6. persist state
7. close database
8. exit workers
9. exit runtime

---

# 128. Resource Rules

Hard engineering rules:

1. No heavyweight model required for idle.
2. No unnecessary screenshots.
3. No large model for deterministic actions.
4. No polling when events exist.
5. No MCP hop without benefit.
6. No unnecessary model reloads.
7. No permanent high-FPS companion.
8. No unnecessary audio-device reopen.
9. Cancel obsolete work.
10. Prefer streaming.

---

# 129. User Onboarding

The onboarding should feel like a premium product setup, not a developer installer.

---

## Step 1 — Welcome

Explain:

> “KIVO is your AI interface for Windows.”

---

## Step 2 — Hardware

Detect system capabilities.

---

## Step 3 — Brain

Choose:

- API
- CLI
- Local
- Login
- Later

---

## Step 4 — Voice

Choose STT and TTS.

---

## Step 5 — Wake Word

Choose:

- wake word
- push-to-talk
- both

---

## Step 6 — Permissions

Explain each permission.

---

## Step 7 — Companion

Enable/disable.

---

## Step 8 — Performance

Choose:

- Battery
- Balanced
- Performance
- Custom

---

# 130. Onboarding Recommendation Engine

KIVO should recommend settings based on:

- hardware
- installed software
- available GPU
- available RAM
- network
- provider configuration

The user can override recommendations.

---

# 131. Brain Onboarding Example

```text
How should KIVO think?

[ Cloud / API ]
Use your preferred AI provider.

[ CLI / Agent ]
Use an AI agent already installed on your PC.

[ Local ]
Run a model locally.

[ I'll decide later ]
Continue without configuring a brain.
```

---

# 132. Voice Onboarding Example

```text
How should KIVO understand you?

⚡ Fast
Low resources

⚖ Balanced
Speed + accuracy

🎯 Accurate
Higher resource usage

☁ Cloud
Provider-based speech recognition
```

---

# 133. Voice Output Onboarding

```text
How should KIVO speak?

⚡ Instant
Fastest response

⚖ Natural
Balanced

🎙 Expressive
Higher-quality voice

🖥 System
Use Windows voices
```

---

# 134. Control Center Design System

Define:

- spacing scale
- typography scale
- icon rules
- corner radius
- elevation
- motion
- color system
- dark/light themes
- accessibility states

The visual language must be consistent across every screen.

---

# 135. Motion Design

Animations should:

- communicate state
- confirm action
- guide attention
- avoid distraction

Use reduced motion when requested.

---

# 136. Companion and Control Center Relationship

The Companion is an ambient surface.

The Control Center is the management surface.

The Runtime is the actual product core.

---

# 137. Example Complete Interaction

User:

> “Hey Kivo, open my coding project, find why the tests are failing, fix it, and tell me what you changed.”

KIVO should:

1. detect wake word
2. activate STT
3. transcribe progressively
4. classify as complex coding task
5. select Coding brain profile
6. identify project
7. obtain required permissions
8. open project
9. inspect state
10. run tests
11. capture failure
12. reason about failure
13. modify files
14. run tests
15. validate
16. summarize changes
17. stream response
18. speak
19. show activity
20. preserve relevant task history

---

# 138. Example Fast Interaction

User:

> “Kivo, mute.”

KIVO should:

1. wake
2. transcribe
3. classify deterministic command
4. execute native audio action
5. confirm briefly

No external AI request.

---

# 139. Example Private Interaction

User:

> “Summarize this confidential document locally.”

KIVO:

1. detects privacy requirement
2. blocks cloud providers
3. selects local model
4. processes document locally
5. returns summary

---

# 140. Example Background Task

User:

> “Tell me when the build finishes.”

KIVO:

1. identifies build/process
2. creates watcher
3. registers event
4. stops active LLM usage
5. waits
6. receives completion event
7. notifies user

---

# 141. Example Visual Assistance

User:

> “Where is the Export button?”

KIVO:

1. identifies active application
2. queries UI Automation
3. locates Export control
4. obtains bounds
5. companion points
6. voice explains

---

# 142. Product Modes

## Normal

Balanced operation.

## Private

Prefer local.

## Offline

No cloud.

## Battery

Minimal resources.

## Performance

Aggressive prewarming.

## Gaming

Reduce GPU interference.

## Presentation

Companion and UI optimized for demonstration.

---

# 143. User Preferences

Users can configure:

- verbosity
- voice
- language
- wake word
- confirmation policy
- provider
- model
- privacy
- companion
- performance
- notification behavior

---

# 144. Conversational Style

Default KIVO responses should be:

- concise
- useful
- context-aware
- non-repetitive

For actions:

> “Done.”

For complex tasks:

> “I found the failing test. The issue was an incorrect path assumption. I fixed it and the tests now pass.”

Avoid unnecessary narration.

---

# 145. Explainability

KIVO should expose concise operational explanations.

Example:

> “I used your Coding brain because this was detected as a software-development task.”

Do not expose hidden chain-of-thought.

Expose:

- selected provider
- selected tool
- permission decision
- result

---

# 146. Error UX

Bad:

> Error 0x800...

Better:

> “Chrome could not be opened because Windows reported that the application is unavailable.”

Then provide:

> “Retry” / “Open settings”

---

# 147. Future Multi-Agent System

KIVO can eventually orchestrate multiple agents.

Example:

```text
KIVO Brain
   │
   ├── Research Agent
   ├── Coding Agent
   ├── Browser Agent
   └── Local Private Agent
```

KIVO remains the coordinator.

---

# 148. Future Workflow Builder

Users may eventually visually construct:

```text
Trigger
 ↓
Condition
 ↓
Action
 ↓
AI step
 ↓
Validation
 ↓
Notification
```

This should reuse the same Tool and Task systems rather than introducing a second automation engine.

---

# 149. Future Plugin Marketplace

Possible categories:

- AI providers
- voices
- applications
- MCP integrations
- workflows
- companion skins
- tools

All plugins should declare permissions.

---

# 150. Future Voice Marketplace

Users could install:

- voices
- languages
- styles
- pronunciation packs

Only legally permitted voice technology should be supported.

---

# 151. Future Cross-Device Architecture

Potential future:

```text
KIVO Desktop
     │
     ├── KIVO Mobile
     ├── KIVO Web
     └── KIVO Remote
```

The desktop runtime remains the primary computer-control authority.

---

# 152. Future Home/IoT Integration

The Tool architecture should allow:

- smart home
- network devices
- media systems
- cameras where authorized
- IoT

These should remain permission-controlled.

---

# 153. Future Distributed Architecture

Long-term:

```text
KIVO Runtime
     │
 ┌───┼───────────┐
 │   │           │
PC  Phone     Cloud
```

But the first release should remain focused on Windows.

---

# 154. Development Methodology

Build vertically rather than building every subsystem independently.

A useful milestone is:

> User speaks → KIVO understands → KIVO performs one useful action → KIVO speaks back.

Then expand capabilities.

---

# 155. Development Order

## Phase 0 — Specification

- freeze architecture
- define interfaces
- define state model
- define event model
- define security model
- define benchmark requirements

## Phase 1 — Runtime

- Rust core
- lifecycle
- event bus
- IPC
- configuration

## Phase 2 — Voice

- audio
- VAD
- wake
- STT
- TTS
- streaming
- interruption

## Phase 3 — Fast Actions

- native Windows actions
- applications
- windows
- audio
- files

## Phase 4 — Brain System

- API adapters
- CLI adapters
- local adapters
- provider profiles
- health/fallback

## Phase 5 — Computer Control

- UIA
- browser
- app integrations
- input fallback
- vision fallback

## Phase 6 — Agents

- planner
- task graph
- validation
- cancellation
- long-running tasks

## Phase 7 — MCP

- client
- manager
- permissions
- dynamic discovery

## Phase 8 — UI

- onboarding
- Control Center
- settings
- activity
- performance

## Phase 9 — Companion

- overlay
- animations
- pointing
- state visualization

## Phase 10 — Security

- injection defenses
- permission hardening
- credentials
- audit
- emergency stop

## Phase 11 — Performance

- benchmarks
- profiling
- model residency
- hardware optimization

## Phase 12 — Release

- MSI
- updater
- diagnostics
- beta
- stable

---

# 156. MVP Definition

The MVP should include:

- Windows runtime
- wake word
- push-to-talk
- one STT engine
- one TTS engine
- one cloud brain
- one local brain
- one CLI/agent backend
- fast native commands
- application launch
- filesystem tools
- browser launch
- basic UI Automation
- basic permissions
- Control Center
- logging
- cancellation
- basic companion

---

# 157. Beta Definition

Beta should additionally include:

- multiple cloud providers
- provider profiles
- CLI discovery
- dependency management
- multiple STT/TTS choices
- browser automation
- MCP
- background tasks
- task history
- privacy modes
- advanced permission system
- benchmarking
- provider fallback
- polished companion

---

# 158. Production Definition

Production requires:

- stable runtime
- hardened security
- reliable recovery
- updater
- rollback
- diagnostic system
- broad provider support
- robust computer control
- performance optimization
- accessibility
- documentation
- automated testing
- safe defaults

---

# 159. Quality Gates

A release should not ship unless:

### Stability

No known critical crashes.

### Security

No known critical permission bypass.

### Performance

Idle resource consumption is within defined budget.

### Voice

Wake and speech interaction are reliable.

### Computer control

Core Windows actions are deterministic.

### AI

Provider failure does not crash the application.

### UX

Onboarding is understandable without technical knowledge.

---

# 160. Architecture Invariants

These rules should not be violated without an explicit architecture review:

1. Runtime remains provider-independent.
2. UI is not the core runtime.
3. Heavy AI is not required for deterministic actions.
4. Permissions cannot be bypassed by AI output.
5. Native control is preferred over vision.
6. MCP is not mandatory for every internal action.
7. Voice supports interruption.
8. Long-running tasks are cancellable.
9. Provider failure is isolated.
10. Secrets are isolated from normal model context.
11. User can disable cloud processing.
12. The system can operate in a useful offline mode where local capabilities exist.

---

# 161. Final Architecture Diagram

```text
                         K.I.V.O.
              Knowledge • Intelligence •
                  Voice • Operations
                              │
       ┌──────────────────────┼──────────────────────┐
       │                      │                      │
       ▼                      ▼                      ▼
   VOICE SYSTEM          BRAIN SYSTEM          CONTROL CENTER
       │                      │                      │
 ┌─────┼─────┐         ┌──────┼──────┐               │
 │     │     │         │      │      │               │
VAD  Wake   STT       API    CLI   Local             │
 │           │         │      │      │               │
 └─────┬─────┘         └──────┼──────┘               │
       │                       │                      │
       ▼                       ▼                      │
               ┌────────────────────────┐             │
               │     INTENT ROUTER       │◄────────────┘
               └───────────┬────────────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
              ▼            ▼            ▼
           FAST PATH     BRAIN PATH   EVENT PATH
              │            │            │
              │            ▼            │
              │         PLANNER         │
              │            │            │
              └────────────┼────────────┘
                           ▼
                    ┌──────────────┐
                    │ TOOL ROUTER  │
                    └──────┬───────┘
                           │
       ┌───────────────────┼────────────────────┐
       │                   │                    │
       ▼                   ▼                    ▼
    NATIVE                UIA                BROWSER
       │                   │                    │
       └───────────────────┼────────────────────┘
                           │
                    ┌──────┼──────┐
                    │             │
                    ▼             ▼
                  VISION      INPUT FALLBACK
                    │             │
                    └──────┬──────┘
                           ▼
                        WINDOWS
                           │
                           ▼
                     RESULT/EVENT
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
             TTS       COMPANION     ACTIVITY
```

---

# 162. The Core KIVO Loop

The entire product should optimize this loop:

```text
User
 ↓
Wake
 ↓
Speech
 ↓
Streaming STT
 ↓
Intent
 ↓
Fast Path OR Brain
 ↓
Tool
 ↓
Validation
 ↓
Streaming TTS
 ↓
User
```

Every engineering decision should be evaluated against this loop.

---

# 163. The Ultimate Design Rule

KIVO should behave according to this hierarchy:

```text
Can Windows do it directly?
        │
       YES
        ↓
    Do it directly.

        │ NO
        ▼

Can an application API/CLI do it?
        │
       YES
        ↓
    Use the API/CLI.

        │ NO
        ▼

Can UI Automation do it?
        │
       YES
        ↓
       Use UIA.

        │ NO
        ▼

Can the browser DOM/accessibility layer do it?
        │
       YES
        ↓
     Use browser semantics.

        │ NO
        ▼

Can vision identify the target?
        │
       YES
        ↓
       Use vision.

        │ NO
        ▼

Use mouse/keyboard fallback.
```

And separately:

```text
Does this request require intelligence?
        │
       NO
        ↓
   Fast deterministic path.

        │ YES
        ▼
    Brain Router.

        │
        ├── Cloud API
        ├── CLI Agent
        ├── Local Brain
        └── Other Provider
```

---

# 164. Final Product Definition

KIVO is:

> **A lightweight, voice-first, provider-independent AI runtime for Windows that combines local voice processing, interchangeable AI brains, native computer capabilities, semantic UI automation, browser automation, external tools, and optional visual embodiment into one coherent personal assistant.**

It is designed so that:

- the user does not need to care which model is underneath,
- the application does not depend on one AI vendor,
- simple commands do not require expensive AI,
- complex tasks can use powerful external agents,
- private tasks can remain local,
- the computer is controlled semantically before visually,
- MCP remains an interoperability mechanism,
- voice feels immediate,
- long tasks remain observable and cancellable,
- the companion remains optional,
- and the runtime remains lightweight even when powerful AI capabilities are available.

---

# 165. Final Engineering Philosophy

The project should always favor:

**Speed over unnecessary complexity.**

**Native capabilities over simulation.**

**Events over polling.**

**Streaming over waiting.**

**Deterministic execution over hallucinated actions.**

**Provider abstraction over vendor lock-in.**

**Local processing when it improves speed or privacy.**

**Cloud intelligence when it provides meaningful capability.**

**Small resident processes over permanently loaded heavyweight models.**

**Explicit permissions over implicit trust.**

**Measured performance over assumptions.**

**A beautiful interface over a cluttered dashboard.**

**User control over hidden autonomy.**

---

# 166. Final One-Sentence Definition

> **KIVO is the intelligent voice and operations layer between a human and their Windows computer.**

That is the product we are building.

---

# 167. Amendments (2026-09-21)

Phase 0 research changed or refined these parts of the blueprint. The detailed specs are authoritative for the items below.

| Section | Amendment | Source |
|---|---|---|
| §5, §153 | **Platform:** Windows 11 first and most polished; Windows 10 supported with fallbacks; **macOS and Linux planned**, with all OS code behind platform traits from day one | [DECISIONS.md](DECISIONS.md) |
| §8 | **Process model:** `kivo-runtime` (core, tray, audio, wake) + `kivo-app` (Tauri UI; restartable) + `kivo-infer` (supervised model workers). The runtime is a per-user process, not a Windows service | [ARCHITECTURE.md](architecture/ARCHITECTURE.md) |
| §12 Type B/D | **CLI agents are integrated through the Agent Client Protocol (ACP)**, with the Codex App Server as an optional native adapter. Managed login only goes through providers' own CLIs/agents or official OAuth | [BRAINS.md](architecture/BRAINS.md) |
| §23, §27 | **Picovoice (Porcupine/Eagle/Orca) is excluded** (enterprise-only since 2026-06-30). Wake word: a KIVO-trained "Hey Kivo" model + sherpa-onnx keyword spotting for **user-defined custom wake words** | [VOICE.md](architecture/VOICE.md) |
| §32 | Orca is removed from the TTS options. Defaults: Kokoro (Natural), Supertonic (Instant), system voices, and cloud. **GPL espeak-ng/Piper only as optional add-ons** | [VOICE.md](architecture/VOICE.md) |
| §27, §28 | Activation: the **"Hey Kivo" wake word** + **Ctrl+Space push-to-talk**; Siri-style **voice enrollment** (speaker verification is a convenience filter, never an authorization) | [DECISIONS.md](DECISIONS.md) |
| §74–78 | **Voice overlay: "Island"**, a black capsule at the top center that morphs into a transcript/answer card and then a live activity, with an optional wake **edge glow** (off by default); the overlay style can be turned off | [UX.md](architecture/UX.md) |
| §49 | **Browser:** a KIVO browser extension + native messaging for the user's real browser (Chrome 136+ blocks CDP on default profiles); CDP only on a KIVO-managed profile | [TOOLS_AND_CONTROL.md](architecture/TOOLS_AND_CONTROL.md) |
| §60 | **Prompt-injection defense:** provenance/taint tracking + destination binding at the tool boundary ("CaMeL-lite") in v1; full plan-interpreter later | [SECURITY.md](architecture/SECURITY.md) |
| §21, §110 | **Installer:** NSIS per-user is primary; MSI for IT; MSIX/package identity is a later spike (it unlocks the Windows AI Speech API) | [DISTRIBUTION.md](architecture/DISTRIBUTION.md) |
| §155 | Phases are refined into vertical milestones M0–M9 with exit criteria | [ROADMAP.md](ROADMAP.md) |
| §58, new | **Capabilities center:** every capability can be toggled, and disabled means removed. Includes opt-in **computer use** and **screen awareness** | [CAPABILITIES.md](architecture/CAPABILITIES.md) |
| §148, new | **Routines and custom commands** (no AI required), built on the Task engine | [ROUTINES.md](architecture/ROUTINES.md) |
| §12, new | **Realtime speech-to-speech** conversation mode; **usage/cost tracking with optional user limits**; **personas** | [BRAINS.md §8–10](architecture/BRAINS.md) |
| §74–78 | Companion styles: Pill, Orb, Character or Hidden, switchable | [UX.md §6](architecture/UX.md) |
| §121, §151–152 | Integration strategy, WASM plugins, KIVO Remote | [INTEGRATIONS_AND_PLUGINS.md](architecture/INTEGRATIONS_AND_PLUGINS.md) |
| §22 | No bundled local LLM; users connect their own local servers | [DECISIONS.md](DECISIONS.md) |
| §2 | The name **KIVO is kept** (owner decision), despite existing "KIVO" AI products; check the registries before public release | [research §8](research/features-and-extensions/REPORT.md) |
| §79, §134–135 | **Look:** the Island theme, **light by default** (Light / Dark / System), ink primary buttons, accent for status only, system font, custom title bar. Supersedes "dark-first" | [DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md) |
| §80 | **Navigation:** 13 items with in-page tabs (Home, Chat, Tasks, Activity, Routines · Brains, Agents, Voice, Extensions · Permissions, Memory, Usage · Settings). Tools merged into Capabilities; MCP and Integrations became Extensions tabs; Companion, Performance, Diagnostics and About became Settings tabs | [UX.md §3](architecture/UX.md) |
| §58 | **Permission modes** (Ask / Accept edits / Plan / Auto / Bypass) replace the Maximum Safety / Balanced / Power User profiles | [SECURITY.md §1.1](architecture/SECURITY.md) |
| §62–65, §82 | **Conversation, context and memory v2:** automatic sessions and threads, per-model context budgets with compaction and prompt caching, instructions and workspaces, an Obsidian-compatible memory vault with Suggest + Workspace notes, skills, context layers | [CONVERSATION.md](architecture/CONVERSATION.md) |
| §18–20 | **Discovery:** CLIs, local servers, desktop AI apps, MCP configs and skills are detected and suggested; in-app installs with consent; data-freshness rules | [DISCOVERY.md](architecture/DISCOVERY.md) |
| §112–113 | **Release pipeline:** GitHub Actions, release-please, `tauri-action` matrix, Stable/Beta/Experimental | [RELEASE.md](architecture/RELEASE.md) |
| §129–133 | **Onboarding:** 11 steps in 5 phases (Welcome · Voice · Brain · Control · Ready), replacing the 8 steps | [UX.md §4](architecture/UX.md) |
| §74 | Companion "modes" (hidden/ambient/compact/full/presentation) are replaced by companion **styles** (§74–78 row above) plus the Island's own states; "Presentation" survives as a product mode (PLAN-06) | [UX.md §2, §6](architecture/UX.md) |
| §155 | **Build tracking:** every requirement has an ID and a status mark in its spec's build checklist; §168 maps each plan section to those IDs | [docs/README.md](README.md) |

---

# 168. Coverage map and plan-only requirements (2026-09-21)

This section ties every part of the plan to the checklist items that build it, so nothing in the
plan is lost. Item IDs and status marks are explained in [docs/README.md](README.md). Sections
that state philosophy or examples are covered by the rules and acceptance items listed.

## 168.1 Coverage map

| Plan § | Topic | Built by | Milestone |
|---|---|---|---|
| 1–6 | Summary, name, philosophy, goals, non-goals, target UX | Principles enforced through ARCH §8 invariants and the items below; PLAN-19–23 acceptance journeys | All |
| 7–10 | System architecture, processes, technology, Rust | ARCH-01–13, ARCH-33–36 | M0 |
| 11–16 | Brain architecture, provider types, contract, profiles, routing | BRAIN-08–23 | M3 |
| 17 | API configuration UX | UX-22, BRAIN-17–18 | M3 |
| 18–20 | CLI discovery and installation, dependency manager | DISC-04, DISC-07, DIST-14 | M3, M8 |
| 21 | Base installer philosophy | DIST-01–05 | M1, M9 |
| 22 | Local AI | BRAIN-12, DISC-05 | M3 |
| 23–26 | STT architecture, selection UX, hardware detection, residency | VOICE-06, VOICE-08, VOICE-10, VOICE-34, UX-23, PLAN-01, PLAN-02 | M1, M8 |
| 27–31 | Wake word, push-to-talk, VAD, endpointing, streaming STT | VOICE-04–05, VOICE-13–19, VOICE-33, UX-41 | M1, M2 |
| 32–35 | TTS architecture, selection, streaming, barge-in | VOICE-09, VOICE-11, BRAIN-28, VOICE-31 | M1–M3, M8 |
| 36 | Voice personality | BRAIN-29, BRAIN-38, PLAN-11 | M1, M3 |
| 37–38 | Fast intent router, deterministic fast path | BRAIN-01–05, TOOL-06–18 | M1, M4 |
| 39–42 | Agent path, planner, task graph, validation | BRAIN-30–32, ARCH-27 | M5 |
| 43 | Cancellation | ARCH-25–26, PLAN-03 | M1, M5 |
| 44–46 | Tool router, capability ladder, native tools | TOOL-04–18 | M1, M4 |
| 47–48 | UI Automation and events | TOOL-19–22 | M4 |
| 49–50 | Browser control and context | TOOL-23–27 | M1, M4 |
| 51–52 | Vision, mouse/keyboard | TOOL-32–33, CAP-08–13 | M4, M8 |
| 53–55 | MCP, manager, dynamic exposure | TOOL-34–37, UX-27, BRAIN-27 | M6 |
| 56–58 | Risk classification, permission engine, profiles → modes | TOOL-01–02, SEC-01–12 | M1, M4, M5 |
| 59 | Shell access | TOOL-30–31 | M4 |
| 60 | Prompt-injection defense | SEC-13–16 | M4, Post |
| 61 | Credential security | SEC-17–19 | M0, M3 |
| 62–65 | Memory, rules, context, deltas | MEM-01–11, CONV-01–07, CONV-17–25, CONV-30–31 | M3, M7, M8 |
| 66 | Event bus | ARCH-22–23 | M0, M1 |
| 67–68 | Background automation, scheduler | TOOL-28–29, ROUT-11 | M5, M8 |
| 69–70 | Privacy modes, data classification | SEC-20–21 | M7 |
| 71 | Offline mode | PLAN-04 | M7 |
| 72–73 | Network resilience, failover | PLAN-05, BRAIN-22 | M3 |
| 74–78 | Companion, states, visuals, targeting, rendering | UX-05–17, UX-38–39, DS-09 | M0–M2, M8 |
| 79 | UI/UX philosophy | DS-01–16 | D0, M7 |
| 80–88 | Control Center pages | UX-18–32, CAP-04 | M1–M7 |
| 89 | Performance dashboard | PLAN-07 | M8 |
| 90–92 | Hardware profiles, GPU policy, NPU | PLAN-08, PLAN-09, VOICE-35, VOICE-12 | M8, Post |
| 93–95 | Model residency, prewarming, fast acknowledgement | PLAN-02, VOICE-34, PLAN-10, PLAN-11 | M1, M3 |
| 96–103 | Performance philosophy, instrumentation, benchmarks, targets | ARCH-28, BENCH-01–14, VOICE-40 | M0, M1 |
| 104–106 | Security architecture, emergency stop, audit | SEC-22–27 | M1, M2, M4 |
| 107–108 | Configuration, database | ARCH-29–31 | M0 |
| 109 | Diagnostics | ARCH-40, SEC-23 | M8 |
| 110–113 | Installer, dependency installer, updates, channels | DIST-01–14, REL-01–12 | M0, M1, M8, M9 |
| 114 | Crash recovery | ARCH-03, ARCH-09, ARCH-27, PLAN-12 | M0–M8 |
| 115 | Testing strategy | PLAN-13, TOOL-38–41, BENCH-13 | M0 onward |
| 116–117 | Test environment, security testing | TOOL-38–41 | M4, M6 |
| 118–119 | Accessibility, internationalization | UX-49–55, VOICE-36–39, DS-16 | M1, M7, L1–L5 |
| 120 | Application capability registry | TOOL-05 | M4 |
| 121 | Plugin architecture | INT-09–10 | M4, Post |
| 122–123 | Tool schema, dynamic tool selection | TOOL-01, BRAIN-27 | M1, M3 |
| 124–125 | State synchronization, IPC | ARCH-14–21, ARCH-39 | M0, M1 |
| 126–127 | Startup and shutdown sequences | PLAN-14, PLAN-15 | M1 |
| 128 | Resource rules | PLAN-16 | M1 onward |
| 129–133 | Onboarding and recommendations | UX-33–36 | M2, M3, M7 |
| 134–136 | Design system, motion, surfaces | DS-01–16 | D0, M7 |
| 137–141 | Example interactions | PLAN-19–23 | M1–M8 |
| 142–143 | Product modes, user preferences | PLAN-06, UX-37 | M7, M8 |
| 144–146 | Conversational style, explainability, error UX | BRAIN-29, PLAN-17, PLAN-18, TOOL-03 | M1, M3 |
| 147–153 | Future: multi-agent, workflow builder, marketplaces, cross-device, IoT, distributed | ROUT-*, INT-10–14, PLAN-24–26 | M5, M8, Post |
| 154–155 | Methodology, development order | ROADMAP.md, docs/README.md | — |
| 156–159 | MVP, beta, production definitions, quality gates | PLAN-27–30 | M7, M8, M9 |
| 160 | Architecture invariants | ARCH §8, PLAN-31 | M9 |
| 161–166 | Final diagrams and definitions | Reference only | — |

## 168.2 Plan-only requirements

- [x] **PLAN-01** · M1 · Hardware detection through `SystemInfo`: CPU, RAM, GPU, VRAM, NPU where supported, battery state and current load; feeds the voice recommendation (§25, §90) → done: `SystemInfo::snapshot` (CPU name and threads, RAM total and free, GPUs with VRAM, battery, CPU load) and `detect_capabilities` (NPU through DXCore where present) feed `kivo_voice::recommend`, which sets the speech engines' threads · verified: `machines_fall_into_the_reference_tiers`, `battery_and_load_hold_the_models_back`, live hardware advice (2026-09-22)
- [x] **PLAN-02** · M1 · Model residency states Unloaded → done: the worker reports each engine Unloaded → Warming → Warm → Active → Idle → Unloading (STT and TTS); the runtime keeps them per engine, publishes `ModelResidency` events and puts the state on each model in `models.list`; the Voice page shows it · verified: `residency_changes_are_kept_and_published`, `models_load_on_use_and_unload_after_the_warm_time` (2026-09-23)
- [ ] **PLAN-03** · M5 · Cancellation also propagates from task timeouts, provider failures and application shutdown (§43)
- [ ] **PLAN-04** · M7 · Offline mode verified end to end: wake, STT, TTS, native commands, files, UIA, local tools, local brain and local automation all work with the network off (§71)
- [ ] **PLAN-05** · M3 · Network resilience: offline, timeout, provider failure, rate limit, auth failure and degraded connection each produce a useful fallback and a plain message (§72)
- [ ] **PLAN-06** · M8 · Product modes Normal, Private, Offline, Battery, Performance, Gaming, Presentation, each mapped onto privacy mode, performance profile and Island behaviour (§142)
- [ ] **PLAN-07** · M8 · Performance dashboard: CPU, RAM, GPU, VRAM, NPU, model residency, and STT / brain / tool / TTS / total latency (§89)
- [ ] **PLAN-08** · M8 · Performance profiles Low Resource, Balanced, Performance, Battery, Gaming, Custom, chosen automatically with manual override (§90)
- [ ] **PLAN-09** · M8 · GPU policy: consider GPU load, VRAM, active apps, battery, gaming and model needs; never compete with GPU-heavy work (§91)
- [ ] **PLAN-10** · M3 · Predictive prewarming after wake: prepare STT, the likely tool subsystem and optionally the likely provider connection, never with side effects (§94)
- [x] **PLAN-11** · M1 · Fast acknowledgement: long operations show immediate visual feedback in the Island, with no spoken filler (§36, §95) → done: the Island changes the moment KIVO moves on (listening with the live transcript, thinking, working with its steps) and the listening cue plays at once; no spoken filler (the optional thinking cue is a sound, off by default) · verified: press → cue 6 ms and the Island states in the end-to-end tests, turn tests (2026-09-23)
- [ ] **PLAN-12** · M8 · Crash recovery: a crashed worker is isolated and restarted where safe, task state is preserved, the problem is reported, failures do not cascade (§114)
- [~] **PLAN-13** · M0 · Test layers in place: unit tests (routing, permissions, state, config, adapters, task graph) from M0; integration tests as each subsystem lands; end-to-end tests for voice → partial: unit tests across kivo-core, kivo-ipc (incl. end-to-end over the real pipe), kivo-store, kivo-platform-windows (real hotkeys, audio, tray), kivo-runtime, kivo-bench and the UI (Vitest) · missing: integration and end-to-end voice tests as the M1 pipeline lands
- [x] **PLAN-14** · M1 · Startup sequence: runtime → done: `kivo-runtime` starts in the §126 order: single instance, settings, logging, the event bus and state, IPC, audio and voice detection, then OS registrations (tray, hotkeys, autostart, notifications); speech models load only when a request needs them · verified: startup reading of `main.rs` against §126, cold start and idle measured live (2026-09-23)
- [x] **PLAN-15** · M1 · Shutdown sequence: stop new tasks, notify and cancel active work, stop audio and providers, persist state, close the database, exit workers, then the runtime (§127) → done: on quit the UIs are told (`ShuttingDown`), the current turn is cancelled, the voice pipeline and the speech worker stop, the IPC server, tray, hotkeys and app supervisor end, the store closes and the session token is removed (§127) · verified: `an_event_published_just_before_shutdown_is_still_delivered`, live quits from the tray (2026-09-23)
- [x] **PLAN-16** · M1 · The ten resource rules are reviewed at each milestone exit, with any exception logged in DECISIONS.md (§128) → done: the ten rules reviewed at M1 exit, with the polling found fixed and the three exceptions logged (DECISIONS "Resource rules at M1 exit") · verified: the review and the idle measurement (2026-09-23); repeated at each milestone exit
- [ ] **PLAN-17** · M3 · Explainability: the card and Activity show the selected provider, tool, permission decision and result, never hidden chain-of-thought (§145)
- [x] **PLAN-18** · M1 · Error UX: plain-language cause plus Retry / Open settings actions, never raw codes (§146) → done: failures read as a plain cause plus what to do ("… Try saying the name a different way", "… You may need to allow it in Windows"), with Retry and Open in Control Center on the Island, Open Windows settings / Type instead for a blocked mic; raw codes only in the log · verified: `failures_are_worded_for_people`, turn and lifecycle tests (2026-09-23)
- [x] **PLAN-19** · M1 · Acceptance journey §138 "Kivo, mute": no external AI request, brief confirmation → done: "Kivo, mute" said aloud is transcribed, routed by the grammar (no AI), mutes the speakers and answers "Muted." · verified: `saying_mute_mutes_with_no_ai_and_a_brief_answer`, `a_typed_command_is_treated_like_a_spoken_one` (2026-09-23)
- [ ] **PLAN-20** · M5 · Acceptance journey §137 (coding project: open, find failing test, fix, validate, summarize, speak)
- [ ] **PLAN-21** · M7 · Acceptance journey §139 (confidential document summarized with a local model, cloud blocked)
- [ ] **PLAN-22** · M5 · Acceptance journey §140 (build watcher: no LLM while waiting, notify on completion)
- [ ] **PLAN-23** · M8 · Acceptance journey §141 (where is the Export button: UIA bounds, companion points, voice explains)
- [ ] **PLAN-24** · Post · Multi-agent coordination with KIVO as the coordinator (§147)
- [ ] **PLAN-25** · Post · Voice marketplace: voices, languages, styles, pronunciation packs, legally permitted only (§150)
- [ ] **PLAN-26** · Post · Home/IoT tools under the same permission model (§152)
- [ ] **PLAN-27** · M7 · MVP definition (§156) fully met → internal alpha
- [ ] **PLAN-28** · M8 · Beta definition (§157) fully met
- [ ] **PLAN-29** · M9 · Production definition (§158) fully met
- [ ] **PLAN-30** · M9 · Quality gates (§159) pass for the release
- [ ] **PLAN-31** · M9 · Architecture invariants checklist (§160, ARCHITECTURE §8) all green
