# VoiceForge Command Interface

All app operations go through this interface. The TUI translates key events into commands. A future REST API sends the same commands.

## Engine

```rust
pub struct Engine {
    state: EngineState,
    processing_tx: Sender<ProcessingJob>,
    // ...
}

impl Engine {
    pub fn execute(&mut self, cmd: Command) -> Result<Response, EngineError>;
    pub fn query(&self, q: Query) -> QueryResult;
}
```

## Commands

```rust
pub enum Command {
    // File
    LoadFile { path: PathBuf },
    ExportWav { path: PathBuf },

    // Transport
    Play,
    Pause,
    Stop,
    Seek { position_secs: f64 },
    SetLoop { enabled: bool },

    // A/B
    SetSource { source: Source },  // Original | Processed

    // Parameters — single unified setter
    SetParam { param: ParamId, value: f64 },
    ResetParam { param: ParamId },
    ResetAll,

    // Analysis settings
    SetAnalysisConfig { config: AnalysisConfig },
}

pub enum Source { Original, Processed }
```

## Parameters

```rust
pub enum ParamId {
    // WORLD vocoder
    PitchShift,
    PitchRange,
    Speed,
    Breathiness,
    FormantShift,
    SpectralTilt,
    // Effects
    Gain,
    LowCut,
    HighCut,
    CompressorThresh,
    ReverbMix,
    PitchShiftFx,
}
```

Each param has static metadata:

```rust
pub struct ParamMeta {
    pub id: ParamId,
    pub name: &'static str,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub step: f64,
    pub unit: &'static str,
}
```

## Queries (read-only state inspection)

```rust
pub enum Query {
    State,                    // full snapshot
    ParamValue(ParamId),
    FileInfo,
    PlaybackPosition,
    Spectrum,
    IsProcessing,
}

pub enum QueryResult {
    State(EngineState),
    ParamValue(f64),
    FileInfo(Option<FileInfo>),
    Position { secs: f64, duration: f64 },
    Spectrum(Vec<f32>),       // dB magnitudes
    IsProcessing(bool),
}
```

## Response

```rust
pub enum Response {
    Ok,
    FileLoaded(FileInfo),
    Exported(PathBuf),
    Error(EngineError),
}

pub struct FileInfo {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
}
```

## EngineState (full snapshot, serializable)

```rust
#[derive(Serialize)]
pub struct EngineState {
    pub file: Option<FileInfo>,
    pub transport: TransportState,
    pub source: Source,
    pub params: HashMap<ParamId, f64>,
    pub is_processing: bool,
}

pub struct TransportState {
    pub playing: bool,
    pub position_secs: f64,
    pub duration_secs: f64,
    pub loop_enabled: bool,
}
```

## How it maps to REST (future)

| Endpoint | Method | Command/Query |
|---|---|---|
| `/file` | POST `{path}` | `LoadFile` |
| `/export` | POST `{path}` | `ExportWav` |
| `/transport/play` | POST | `Play` |
| `/transport/pause` | POST | `Pause` |
| `/transport/stop` | POST | `Stop` |
| `/transport/seek` | POST `{position_secs}` | `Seek` |
| `/transport/loop` | PUT `{enabled}` | `SetLoop` |
| `/source` | PUT `{source}` | `SetSource` |
| `/params/:id` | PUT `{value}` | `SetParam` |
| `/params/:id` | DELETE | `ResetParam` |
| `/params` | DELETE | `ResetAll` |
| `/state` | GET | `Query::State` |
| `/spectrum` | GET | `Query::Spectrum` |

## Integration with phases

- **P2**: Create `src/engine.rs` with `Engine`, `Command`, `Query`, `ParamId`, `ParamMeta`. Input handler calls `engine.execute()` instead of mutating state directly.
- **P3–P6**: Processing thread receives jobs from `Engine`. Each `SetParam` / `LoadFile` triggers the appropriate pipeline stage.
- **P7**: `ExportWav` command already in the interface.
- **Future**: Add `axum` or `actix-web` REST server that calls the same `Engine`.
