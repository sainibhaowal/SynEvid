# SynEvid Distributed Tracing Configuration

**Version**: 1.0
**Date**: 2024-11-24
**Applies To**: All SynEvid components

## Overview

SynEvid uses OpenTelemetry for distributed tracing across all components:
- `autopsy-daemon` (Rust)
- `autopsy-cli` (Rust)
- `synevid-web` (TypeScript/Next.js)
- `synevid-electron` (TypeScript/Electron)
- `synevid-mcp` (TypeScript/Node.js)

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌──────────────────┐
│   Clients   │────►│   Daemon    │────►│  OTLP Exporter   │
│ (CLI/TUI/   │     │ (autopsy-   │     │  (OTLP/HTTP)     │
│  Web/Electron)│    │  daemon)    │     └────────┬─────────┘
└─────────────┘     └─────────────┘              ▼
                                      ┌────────────────────┐
                                      │  OTLP Collector    │
                                      │  (OpenTelemetry)   │
                                      └─────────┬──────────┘
                                                ▼
                                    ┌─────────────────────┐
                                    │   Backend          │
                                    │  (Jaeger/Tempo/    │
                                    │   Zipkin/OTLP)     │
                                    └─────────────────────┘
```

## Trace Context Propagation

### W3C Trace Context

All components propagate trace context via standard HTTP headers:

```http
# Request headers
traceparent: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
tracestate: synvid=op_id:abc123,client_type:cli

# For WebSocket/SSE
Sec-WebSocket-Protocol: traceparent, tracestate
```

### Header Format

```
traceparent: version-trace-id-parent-id-flags
# Example: 00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01

tracestate: synvid=op_id:abc123,client_type:cli,repo_id:repo123
```

## Rust Implementation (Daemon & CLI)

### Dependencies

```toml
# Cargo.toml
[dependencies]
opentelemetry = { version = "0.27", features = ["rt_tokio_current_thread"] }
opentelemetry-otlp = { version = "0.20", features = ["tonic"] }
opentelemetry-sdk = { version = "0.27", features = ["trace"] }
tracing-opentelemetry = "0.26"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

### Daemon Tracing Setup

```rust
// crates/autopsy-daemon/src/tracing.rs
use opentelemetry::{
    global,
    sdk::{
        trace::{self, RandomIdGenerator, Sampler},
        Resource,
    },
    KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_tracing() -> Result<opentelemetry_sdk::trace::TracerProvider, Box<dyn std::error::Error>> {
    let otlp_exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:4317".to_string()))
        .with_timeout(std::time::Duration::from_secs(10))
        .build_span_exporter()?;

    let provider = trace::TracerProvider::builder()
        .with_batch_exporter(otlp_exporter)
        .with_sampler(Sampler::ParentBased(Box::new(trace::TraceIdRatioBased::new(
            std::env::var("OTEL_TRACES_SAMPLER_ARG")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0)  // 100% by default
        ))))
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(Resource::new(vec![
            KeyValue::new("service.name", "autopsy-daemon"),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("deployment.environment", 
                std::env::var("DEPLOYMENT_ENV").unwrap_or_else(|_| "development".to_string())),
        ]))
        .build();

    let tracer = provider.tracer("autopsy-daemon");
    
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .json();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(otel_layer)
        .with(fmt_layer)
        .init();

    global::set_tracer_provider(provider.clone());
    
    Ok(provider)
}
```

### Daemon Request Tracing

```rust
// crates/autopsy-daemon/src/handlers/mod.rs
use tracing::{info, instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;

#[instrument(skip(state), fields(
    operation_id = %request.operation_id,
    command_id = %request.command_id,
    repository_id = %request.repository_id,
    client_id = %request.client_id,
    client_type = %request.client_type,
))]
pub async fn handle_command(
    State(state): State<AppState>,
    Json(request): Json<CommandRequest>,
) -> Result<Json<CommandResponse>, AppError> {
    let span = Span::current();
    
    // Add business context to span
    span.set_attribute("synvid.command_id", request.command_id.clone());
    span.set_attribute("synvid.repository_id", request.repository_id.clone());
    span.set_attribute("synvid.client_type", request.client_type.to_string());
    
    if let Some(session_id) = &request.session_id {
        span.set_attribute("synvid.session_id", session_id.clone());
    }
    
    let start = Instant::now();
    let result = execute_command(state, request).await;
    let duration = start.elapsed();
    
    // Record metrics on span
    Span::current().set_attribute("synvid.duration_ms", duration.as_millis() as i64);
    Span::current().set_attribute("synvid.exit_code", result.as_ref().map(|_| 0).unwrap_or(1));
    
    match &result {
        Ok(response) => {
            span.set_attribute("synvid.verdict", response.verdict.to_string());
            span.set_attribute("synvid.coverage", response.coverage_state.to_string());
            if let Some(digest) = &response.blake3_digest {
                span.set_attribute("synvid.blake3_digest", digest.clone());
            }
        }
        Err(e) => {
            span.record_error(e);
            span.set_attribute("synvid.error", e.to_string());
        }
    }
    
    result
}
```

### CLI Tracing

```rust
// crates/autopsy-cli/src/tracing.rs
use opentelemetry::global;
use opentelemetry_sdk::trace::{self, Sampler};
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_cli_tracing() -> Result<trace::TracerProvider, Box<dyn std::error::Error>> {
    let provider = trace::TracerProvider::builder()
        .with_sampler(Sampler::ParentBased(Box::new(trace::TraceIdRatioBased::new(1.0))))
        .with_resource(Resource::new(vec![
            KeyValue::new("service.name", "autopsy-cli"),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
        ]))
        .build();

    let tracer = provider.tracer("autopsy-cli");
    
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    global::set_tracer_provider(provider.clone());
    Ok(provider)
}
```

### CLI Command Tracing

```rust
// crates/autopsy-cli/src/main.rs
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _provider = init_cli_tracing()?;
    
    let cli = Cli::parse();
    let trace_id = generate_trace_id();
    
    // Set trace context for subprocess calls
    std::env::set_var("TRACEPARENT", format!("00-{trace_id}-0000000000000000-01"));
    std::env::set_var("TRACESTATE", format!("synvid=cli_id:{},version:{}", 
        std::env::var("SYNEVID_CLIENT_ID").unwrap_or_else(|_| "default".to_string()),
        env!("CARGO_PKG_VERSION")
    ));
    
    execute(cli).await
}
```

## TypeScript Implementation (Web/Electron/MCP)

### Dependencies

```json
{
  "dependencies": {
    "@opentelemetry/api": "^1.8.0",
    "@opentelemetry/sdk-trace-base": "^1.22.0",
    "@opentelemetry/sdk-trace-web": "^1.22.0",
    "@opentelemetry/exporter-trace-otlp-http": "^0.50.0",
    "@opentelemetry/resources": "^1.22.0",
    "@opentelemetry/semantic-conventions": "^1.22.0",
    "@opentelemetry/context-zone": "^1.22.0"
  }
}
```

### Web UI Tracing Setup

```typescript
// apps/web/src/lib/tracing.ts
import { WebTracerProvider } from '@opentelemetry/sdk-trace-web';
import { OTLPTraceExporter } from '@opentelemetry/exporter-trace-otlp-http';
import { registerInstrumentations } from '@opentelemetry/instrumentation';
import { DocumentLoadInstrumentation } from '@opentelemetry/instrumentation-document-load';
import { FetchInstrumentation } from '@opentelemetry/instrumentation-fetch';
import { XMLHttpRequestInstrumentation } from '@opentelemetry/instrumentation-xml-http-request';
import { ZoneContextManager } from '@opentelemetry/context-zone';
import { Resource } from '@opentelemetry/resources';
import { SemanticResourceAttributes } from '@opentelemetry/semantic-conventions';

export function initWebTracing() {
  const provider = new WebTracerProvider({
    resource: new Resource({
      [SemanticResourceAttributes.SERVICE_NAME]: 'synevid-web',
      [SemanticResourceAttributes.SERVICE_VERSION]: process.env.NEXT_PUBLIC_APP_VERSION || '0.6.0',
      [SemanticResourceAttributes.DEPLOYMENT_ENVIRONMENT]: process.env.NODE_ENV || 'development',
    },
  });

  const exporter = new OTLPTraceExporter({
    url: `${process.env.NEXT_PUBLIC_OTEL_ENDPOINT || 'http://localhost:4318'}/v1/traces`,
  });

  provider.addSpanProcessor(new BatchSpanProcessor(exporter));
  provider.register({
    contextManager: new ZoneContextManager(),
  });

  // Auto-instrument fetch/XHR
  registerInstrumentations({
    instrumentations: [
      new DocumentLoadInstrumentation(),
      new FetchInstrumentation({
        propagateTraceHeaderCorsUrls: [/.*/],
        applyCustomAttributesOnSpan: (span) => {
          span.setAttribute('synvid.client_type', 'web');
        },
      }),
      new XMLHttpRequestInstrumentation({
        propagateTraceHeaderCorsUrls: [/.*/],
      }),
    ],
  });

  return provider;
}
```

### Web API Tracing

```typescript
// apps/web/src/lib/api.ts
import { trace, context } from '@opentelemetry/api';

export class SynevidApiClient {
  private tracer = trace.getTracer('synevid-web');

  async fetchWithTimeout<T>(path: string, options?: RequestInit, timeoutMs = 15000): Promise<T> {
    return this.tracer.startActiveSpan(`HTTP ${options?.method || 'GET'} ${path}`, async (span) => {
      try {
        span.setAttribute('http.method', options?.method || 'GET');
        span.setAttribute('http.url', `${this.baseUrl}${path}`);
        span.setAttribute('synvid.client_type', 'web');
        
        if (this.runtimeLeaseId) {
          span.setAttribute('synvid.lease_id', this.runtimeLeaseId);
        }

        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), timeoutMs);

        const headers: Record<string, string> = {
          'Content-Type': 'application/json',
          'X-Synevid-Client-Id': this.clientId,
          'X-Synevid-Client-Type': 'web',
        };
        
        if (this.runtimeLeaseId) {
          headers['X-Synevid-Lease-Id'] = this.runtimeLeaseId;
        }

        // Propagate trace context
        const carrier: Record<string, string> = {};
        trace.propagation.inject(context.active(), carrier);
        Object.entries(carrier).forEach(([k, v]) => {
          headers[`traceparent`] = v; // or tracestate
        });

        const res = await fetch(`${this.baseUrl}${path}`, {
          ...options,
          signal: controller.signal,
          headers,
        });

        clearTimeout(timeout);
        
        span.setAttribute('http.status_code', res.status);
        
        if (!res.ok) {
          span.recordException(new Error(`HTTP ${res.status}`));
          span.setStatus({ code: SpanStatusCode.ERROR, message: `HTTP ${res.status}` });
        }
        
        span.end();
        return res.json() as Promise<T>;
      } catch (error) {
        span.recordException(error as Error);
        span.setStatus({ code: SpanStatusCode.ERROR, message: (error as Error).message });
        span.end();
        throw error;
      }
    });
  }
}
```

### Electron Tracing

```typescript
// apps/desktop-electron/src/tracing.ts
import { NodeTracerProvider } from '@opentelemetry/sdk-trace-node';
import { OTLPTraceExporter } from '@opentelemetry/exporter-trace-otlp-http';
import { Resource } from '@opentelemetry/resources';
import { SemanticResourceAttributes } from '@opentelemetry/semantic-conventions';
import { registerInstrumentations } from '@opentelemetry/instrumentation';
import { HttpInstrumentation } from '@opentelemetry/instrumentation-http';
import { ExpressInstrumentation } from '@opentelemetry/instrumentation-express';

export function initElectronTracing() {
  const provider = new NodeTracerProvider({
    resource: new Resource({
      [SemanticResourceAttributes.SERVICE_NAME]: 'synevid-electron',
      [SemanticResourceAttributes.SERVICE_VERSION]: process.env.npm_package_version || '0.6.0',
      [SemanticResourceAttributes.DEPLOYMENT_ENVIRONMENT]: process.env.NODE_ENV || 'production',
    },
  });

  const exporter = new OTLPTraceExporter({
    url: `${process.env.OTEL_EXPORTER_OTLP_ENDPOINT || 'http://localhost:4318'}/v1/traces`,
  });

  provider.addSpanProcessor(new BatchSpanProcessor(exporter));
  provider.register();

  registerInstrumentations({
    instrumentations: [
      new HttpInstrumentation(),
      new ExpressInstrumentation(),
    ],
  });

  return provider;
}
```

### MCP Server Tracing

```typescript
// apps/mcp-server/src/tracing.ts
import { NodeTracerProvider } from '@opentelemetry/sdk-trace-node';
import { OTLPTraceExporter } from '@opentelemetry/exporter-trace-otlp-http';
import { Resource } from '@opentelemetry/resources';
import { SemanticResourceAttributes } from '@opentelemetry/semantic-conventions';
import { registerInstrumentations } from '@opentelemetry/instrumentation';

export function initMcpTracing() {
  const provider = new NodeTracerProvider({
    resource: new Resource({
      [SemanticResourceAttributes.SERVICE_NAME]: 'synevid-mcp',
      [SemanticResourceAttributes.SERVICE_VERSION]: '0.6.0',
      [SemanticResourceAttributes.DEPLOYMENT_ENVIRONMENT]: process.env.NODE_ENV || 'production',
    },
  });

  const exporter = new OTLPTraceExporter({
    url: `${process.env.OTEL_EXPORTER_OTLP_ENDPOINT || 'http://localhost:4318'}/v1/traces`,
  });

  provider.addSpanProcessor(new BatchSpanProcessor(exporter));
  provider.register();

  return provider;
}
```

## Context Propagation

### HTTP Headers

```typescript
// Propagate trace context in outgoing requests
function injectTraceContext(headers: Record<string, string>): Record<string, string> {
  const carrier: Record<string, string> = {};
  trace.propagation.inject(context.active(), carrier);
  
  // Standard W3C headers
  if (carrier.traceparent) {
    headers['traceparent'] = carrier.traceparent;
  }
  if (carrier.tracestate) {
    headers['tracestate'] = carrier.tracestate;
  }
  
  // SynEvid custom headers
  if (carrier['synvid-operation-id']) {
    headers['X-Synevid-Operation-Id'] = carrier['synvid-operation-id'];
  }
  
  return headers;
}

// Extract trace context from incoming requests
function extractTraceContext(headers: Record<string, string>) {
  const carrier: Record<string, string> = {};
  
  if (headers.traceparent) carrier.traceparent = headers.traceparent;
  if (headers.tracestate) carrier.tracestate = headers.tracestate;
  if (headers['x-synevid-operation-id']) carrier['synvid-operation-id'] = headers['x-synevid-operation-id'];
  
  return trace.propagation.extract(context.active(), carrier);
}
```

## Sampling Configuration

### Sampling Strategies

```rust
// 100% for errors, 10% for success
let sampler = Sampler::ParentBased(Box::new(
    trace::TraceIdRatioBased::new(0.1)  // 10% of traces
));

// Or: Always sample errors, 10% of successes
let sampler = Sampler::ParentBased(Box::new(
    trace::TraceIdRatioBased::new(0.1)
));
// With custom sampler for errors
let sampler = trace::ParentBasedSampler::new(
    trace::TraceIdRatioBased::new(0.1),
    Some(Box::new(ErrorSampler))  // Custom sampler for errors
);
```

### Environment Variables

```bash
# Sampling rate (0.0 - 1.0)
export OTEL_TRACES_SAMPLER=parentbased_traceidratio
export OTEL_TRACES_SAMPLER_ARG=0.1

# Export endpoint
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318
export OTEL_EXPORTER_OTLP_TRACES_ENDPOINT=http://localhost:4318/v1/traces

# Resource attributes
export OTEL_RESOURCE_ATTRIBUTES=service.name=autopsy-daemon,service.version=0.6.0,deployment.environment=production

# Sampling decision
export OTEL_TRACES_SAMPLER=always_on  # For debugging
export OTEL_TRACES_SAMPLER=always_off # For production (with parent-based)
```

## Trace Context in Logs

### Correlation

```rust
// Include trace ID in structured logs
info!(
    message = "Baseline completed",
    trace_id = %Span::current().context().span().span_context().trace_id(),
    span_id = %Span::current().context().span().span_context().span_id(),
    operation_id = %op_id,
    duration_ms = duration.as_millis(),
);
```

```json
{
  "timestamp": "2024-11-24T14:30:00.123Z",
  "level": "INFO",
  "trace_id": "0af7651916cd43dd8448eb211c80319c",
  "span_id": "b7ad6b7169203331",
  "message": "Baseline completed",
  "fields": {
    "operation_id": "op_abc123",
    "duration_ms": 1234
  }
}
```

## Jaeger/Grafana Tempo Queries

### Common Queries

```logql
# All traces for a repository
{service_name="autopsy-daemon"} | trace_id =~ ".*" | repo_id="abc123"

# Slow operations
{service_name="autopsy-daemon"} | duration > 5s

# Errors
{service_name="autopsy-daemon"} | level="ERROR"

# By operation
{service_name="autopsy-daemon"} | operation_id="impact"

# By client
{service_name="autopsy-daemon"} | client_type="web"
```

---

*Document Version: 1.0*
*Last Updated: 2024-11-24*
*Next Review: 2025-05-24*