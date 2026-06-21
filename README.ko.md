# agentdir

**macOS, Linux, Windows에서 동작하는 Rust 기반 agent-ready 가상 파일 트리 인프라**

[English](README.md) | [한국어](README.ko.md)

[![crates.io](https://img.shields.io/crates/v/agentdir)](https://crates.io/crates/agentdir)
[![PyPI](https://img.shields.io/pypi/v/agentdir)](https://pypi.org/project/agentdir/)
[![npm](https://img.shields.io/npm/v/@nomadamas/agentdir)](https://www.npmjs.com/package/@nomadamas/agentdir)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

agentdir는 원본 파일을 이동하지 않고도 목적에 맞게 재구성된 읽기 전용 폴더 구조로 보여주는 도구입니다. AI 에이전트, 스크립트, 사람이 문서, 미디어, 데이터셋, 생성물, 일반 텍스트, 바이너리 등 운영체제가 볼 수 있는 파일을 작업에 맞는 구조로 탐색할 수 있습니다.

Rust로 만들어졌고 macOS, Linux, Windows에서 동작합니다. APFS, Btrfs, XFS처럼 CoW를 지원하는 파일시스템에서는 대용량 파일을 중복 저장하지 않고 reflink로 다른 레이아웃과 스냅샷을 만들 수 있습니다. CoW를 사용할 수 없으면 바이트 복사 방식으로 materialization을 수행합니다.

핵심은 단순합니다. 사람이 쓰는 원본 파일 구조는 그대로 두고, 에이전트에게는 더 다루기 좋은 작업용 구조를 제공하며, 원본 파일이 바뀌면 두 구조의 매핑을 계속 맞춰두는 것입니다.

---

## 목차

- [agentdir가 필요한 이유](#agentdir가-필요한-이유)
- [설치](#설치)
- [빠른 시작](#빠른-시작)
- [뷰를 동기화 상태로 유지하기](#뷰를-동기화-상태로-유지하기)
- [핵심 개념](#핵심-개념)
- [명령어 참고](#명령어-참고)
- [라이브러리 사용](#라이브러리-사용)
- [작동 방식](#작동-방식)
- [하지 않는 일](#하지-않는-일)

---

## agentdir가 필요한 이유

- **더 나은 에이전트 컨텍스트** — 오래 쌓인 실제 폴더 구조 대신 작업별로 최적화된 파일 레이아웃을 노출합니다.
- **원본 파일은 그대로 유지** — `mv`, `cp`, `rename`, `mkdir`, `rmdir`로 가상 namespace를 재배치해도 소스 파일은 이동하지 않습니다.
- **CoW 파일시스템에서 중복 저장 비용 감소** — 지원되는 파일시스템에서는 큰 PDF, 이미지, 미디어, 데이터셋을 여러 레이아웃에 배치해도 파일 데이터를 복사하지 않습니다.
- **개발 프로젝트에 한정되지 않음** — 문서, 스프레드시트, 프레젠테이션, PDF, 이미지, 미디어, 데이터셋, 일반 텍스트, 바이너리 등 다양한 파일에 사용할 수 있습니다.
- **크로스 플랫폼 네이티브 코어** — Rust 라이브러리, CLI, Python 바인딩, Node.js 바인딩을 제공합니다.

---

## 설치

### Rust

Rust 애플리케이션에 라이브러리를 추가합니다.

```sh
cargo add agentdir
```

CLI를 설치합니다.

```sh
cargo install agentdir-cli
```

설치되는 바이너리 이름은 `agentdir`입니다.

### Python

Python 3.9 이상이 필요합니다.

```sh
pip install agentdir
```

### Node.js

Node.js 18 이상이 필요합니다. 패키지는 `@nomadamas` scope에 있습니다.

```sh
npm install @nomadamas/agentdir
```

사전 빌드 바이너리는 다음 타깃을 지원합니다.

- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`
- `x86_64-unknown-linux-gnu`
- `x86_64-unknown-linux-musl`

---

## 빠른 시작

```sh
# 새 workspace 초기화
agentdir init ./workspace

# 소스 디렉터리를 가상 트리에 매핑
agentdir -w ./workspace map ./team-files /files

# 에이전트가 사용하는 동안 가상 트리를 최신 상태로 유지
agentdir -w ./workspace watch --interval 60

# workspace 상태 확인
agentdir -w ./workspace status

# 가상 namespace 안에서 항목 이동
# 원본 파일은 바뀌지 않습니다.
agentdir -w ./workspace mv /files/q1-report.pdf /reports/q1-report.pdf

# watcher 대신 한 번만 갱신
agentdir -w ./workspace refresh
```

---

## 뷰를 동기화 상태로 유지하기

설치는 라이브러리나 CLI만 설치합니다. 원본 파일을 매핑한 뒤에는 workspace를 어떻게 최신 상태로 유지할지도 선택해야 합니다. 가상 트리는 살아 있는 탐색용 뷰이며, 원본 파일 변경은 reconciliation을 실행할 때 반영됩니다.

CLI 중심으로 사용할 때는 workspace를 소비하는 에이전트나 스크립트 옆에서 watcher를 실행합니다.

```sh
agentdir -w ./workspace watch --interval 60
```

`watch`는 파일시스템 이벤트에 빠르게 반응하고, 놓친 OS 이벤트를 복구하기 위해 주기적인 전체 재스캔도 수행합니다. 이 명령은 foreground에서 실행되므로 계속 켜두려면 process manager, terminal multiplexer, service supervisor, task runner 등에 올려두는 것이 좋습니다.

장시간 watcher를 실행하고 싶지 않다면 에이전트 세션 전, 매핑 export 전, 또는 원하는 일정에 맞춰 `refresh`를 호출합니다.

```sh
agentdir -w ./workspace refresh
```

라이브러리 사용자도 같은 방식으로 동기화를 호출해야 합니다. 소스가 바뀌었을 수 있으면 `Workspace.refresh()`를 호출하고, mtime/size가 바뀌지 않은 파일까지 SHA-256으로 추가 확인하고 싶다면 `refresh_with_hash_verification(true)`를 사용합니다.

---

## 핵심 개념

| 개념 | 의미 |
|------|------|
| Virtual namespace | 하나 이상의 소스 디렉터리에서 만든 작업별 가상 트리 |
| 읽기 전용 materialized view | 가상 트리의 파일은 탐색용입니다. 편집은 원본 소스 경로에서 수행합니다. |
| CoW materialization | APFS, Btrfs, XFS에서는 큰 파일 데이터를 중복하지 않고 clone할 수 있습니다. |
| Reconciliation | `watch` 또는 `refresh`가 소스 변경을 감지하고 가상 트리를 갱신합니다. |
| Snapshots | 동시 작업을 위한 workspace의 격리된 CoW fork입니다. |

## 기능

- **Virtual namespace** — 소스 디렉터리를 임의의 mount point에 매핑한 뒤, 원본을 건드리지 않고 항목을 이동, 복사, 이름 변경할 수 있습니다.
- **CoW materialization** — APFS(macOS), Btrfs/XFS(Linux)에서는 reflink로 파일을 clone하고, NTFS(Windows)에서는 바이트 복사로 fallback합니다.
- **정확한 변경 추적** — metadata(mtime + size)를 기반으로 소스 디렉터리의 추가, 수정, 삭제를 감지하고 가상 트리에 반영합니다.
- **여러 materialization 전략** — `reflink`(기본값), `symlink`, `virtual`
- **스냅샷 지원** — 격리된 동시 workspace를 위한 CoW fork를 제공합니다.
- **파일 포맷 비종속** — 운영체제가 stat할 수 있는 파일이면 문서, PDF, 이미지, 바이너리 등 어떤 파일이든 다룰 수 있습니다.
- **크로스 플랫폼** — macOS, Linux, Windows를 지원하며 내부 가상 경로는 항상 `/`를 사용합니다.
- **세 가지 배포 채널** — Rust 라이브러리, Python 바인딩(PyO3), Node.js 바인딩(NAPI-RS)

---

## 명령어 참고

바이너리 이름은 `agentdir`입니다. 대부분의 명령은 workspace 디렉터리를 지정하는 `-w`/`--workspace <dir>` 플래그를 받습니다. 생략하면 현재 디렉터리를 사용합니다.

| 명령어 | 설명 |
|--------|------|
| `init <path> [--strategy reflink\|symlink\|virtual]` | 새 workspace 초기화 |
| `map <source> <mount>` | 소스 디렉터리를 가상 트리에 매핑 |
| `map-batch --from-json <file>` | JSON 파일 `{"source_path":"virtual_path",...}`에서 batch mapping 적용 |
| `unmap <mount>` | 소스 매핑 제거 |
| `status` | workspace 상태 출력 |
| `stat <path>` | 가상 경로의 metadata 출력 |
| `cat <path>` | 가상 경로로 파일 내용 출력 |
| `refresh` | 소스 변경 감지 및 반영 |
| `mv <from> <to>` | 가상 namespace 안에서 항목 이동 |
| `cp <from> <to>` | 가상 namespace 안에서 항목 복사 |
| `mkdir <path>` | 가상 디렉터리 생성 |
| `rmdir <path> [-r/--recursive]` | 가상 디렉터리 제거 |
| `export-mapping [--format json] [--reverse] [--relative-to <dir>]` | 소스/가상 경로 매핑을 JSON으로 export |
| `watch [-i/--interval <secs>]` | 소스 변경을 감시하고 자동 동기화(foreground, 기본 interval 60초) |

---

## 라이브러리 사용

전체 API 문서는 각 바인딩 README를 참고하세요.

- **Python** — [`bindings/python/README.md`](bindings/python/README.md)
- **Node.js** — [`bindings/node/README.md`](bindings/node/README.md)

Rust 라이브러리 문서는 [docs.rs](https://docs.rs/agentdir)에서 확인할 수 있습니다.

---

## 작동 방식

`map`으로 소스 디렉터리를 매핑하면 agentdir는 atomic JSON manifest에 매핑 정보를 기록합니다. manifest는 write-tmp, fsync, rename 순서로 저장되어 부분 쓰기를 피합니다. `refresh` 또는 background watcher가 실행되면 소스 metadata를 스캔하고 이전 상태와 diff를 계산합니다. 변경된 항목은 CoW clone 또는 CoW를 사용할 수 없는 경우 바이트 복사로 workspace 디렉터리에 materialize됩니다.

가상 namespace는 O(1) lookup을 제공하는 in-memory catalog입니다. 가상 경로는 모든 플랫폼에서 항상 `/`를 separator로 사용합니다.

스냅샷은 workspace 디렉터리의 CoW fork이므로, 지원되는 파일시스템에서는 데이터를 중복하지 않고 동시 작업용 격리 복사본을 만들 수 있습니다.

소스 symlink는 감지하지만 scan 중 따라가지 않습니다.

---

## 하지 않는 일

agentdir는 의도적으로 좁은 범위의 인프라 도구입니다. 다음은 이 프로젝트의 목표가 아닙니다.

- AI/LLM 통합, semantic understanding, intelligent file routing
- 파일 내용 parsing, full-text indexing, search
- 가상 트리를 어떻게 재구성할지 결정하는 orchestrator나 agent
- 파일 포맷 변환 또는 transformation
- dependency graph 분석, AST parsing, 언어별 기능
- access control, permissions, multi-tenancy

---

## 저장소 구조

```text
crates/
  agentdir/         Core Rust library
  agentdir-cli/     CLI binary
bindings/
  python/           Python bindings (PyO3 + maturin)
  node/             Node.js bindings (NAPI-RS)
```

---

## 라이선스

MIT. 자세한 내용은 [LICENSE](LICENSE)를 참고하세요.
