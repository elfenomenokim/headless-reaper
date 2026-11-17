# headless-reaper

[English](https://github.com/elfenomenokim/headless-reaper/blob/main/README.md)

테스트나 자동화 도구가 놓고 간 헤드리스 브라우저를 찾아서 정리합니다. 기본값은 드라이 런입니다.

```console
$ headless-reaper
headless-reaper 0.1.0: 1 orphaned group, 8 processes, 705 MB RSS (998 processes scanned)

ROOT   AGE     RSS     PIDS  PROFILE
13251  1m 12s  705 MB  8     /tmp/hr-readme/profile
       why: headless, temp profile, remote debugging; launcher gone (reparented to launchd 1)

dry run: nothing was killed. Rerun with --kill to terminate these groups.
```

테스트가 헤드리스 크롬을 띄운 뒤 죽거나, 정리 코드가 돌기 전에 `process.exit()`를 부르면 브라우저가 남습니다. 부모가 사라진 브라우저는 init 프로세스로 옮겨 붙고, 탭과 렌더러를 그대로 유지한 채 며칠씩 메모리를 잡고 있습니다. 실제로 한 번은 이렇게 남은 브라우저 프로세스 57개가 24GB쯤을 붙잡고 있었습니다.

이 도구는 프로세스 목록을 읽어 브라우저 한 그루를 대표 프로세스 아래로 묶고, 그 프로세스를 띄운 부모가 사라졌을 때만 정리합니다. 사람이 쓰고 있는 브라우저는 건드리지 않습니다.

## 이렇게 동작합니다

- **부모가 없는 것만.** 부모가 PID 1이거나 init 프로세스이거나, 아니면 이미 없는 PID일 때만 대상으로 봅니다. 부모가 살아 있는 브라우저는 목록에 아예 올리지 않습니다.
- **자동화가 띄운 것만.** `--headless`가 있거나, 임시·자동화용 `--user-data-dir`이 있거나, 그런 프로필에 `--remote-debugging-*`이 함께 있을 때만 대상입니다. 사람이 쓰는 실제 프로필의 브라우저는 `--remote-debugging-port`가 붙어 있어도 대상이 아닙니다.
- **한 그루째 묶어서.** 렌더러, GPU, 유틸리티 프로세스를 대표 프로세스와 함께 묶습니다. 그래서 표시되는 RSS가 그 브라우저가 실제로 쓰는 메모리이고, 한 번에 전부 정리됩니다.
- **안전한 순서.** `--kill`은 그룹의 모든 프로세스에 SIGTERM을 보내고, 기본 10초를 기다린 뒤 살아남은 것에 SIGKILL을 보냅니다. 시그널을 보내기 전에 시작 시각을 비교하므로 PID가 재사용된 프로세스는 건드리지 않습니다.
- **두 가지 구현.** Rust 명령은 macOS에서 libproc/sysctl, Linux에서 `/proc`를 읽습니다. Python 참조 구현은 표준 라이브러리만 씁니다. 조회할 때 브라우저나 네트워크가 필요하지 않습니다.
- **프로필 정리는 선택.** `--remove-profiles`는 정리한 그룹의 임시 프로필을, 임시 경로 안에 있고 살아 있는 브라우저가 쓰지 않고 싱글턴 락도 없을 때만 지웁니다.

## 설치

```console
$ pipx install headless-reaper   # 권장: 격리 설치, PATH에 명령이 생깁니다
$ uvx headless-reaper --help     # 설치 없이 한 번만 실행
$ pip install headless-reaper    # 프로젝트 안에서
```

Python 패키지는 Python 3.9 이상이 필요합니다. macOS와 Linux에서 프로세스를 조회합니다. 두 구현 모두 Windows에서는 미지원 메시지와 종료 코드 2를 반환합니다.

### 한 패키지에 Rust 명령과 Python 라이브러리

maturin의 `bindings = "bin"`으로 만든 wheel은 `headless-reaper` Rust 실행 파일을 설치합니다. 같은 wheel에 `src/headless_reaper`도 들어갑니다. 따라서 `import headless_reaper`와 `python -m headless_reaper`는 기존 Python 참조 구현을 그대로 사용합니다. Rust 실행 파일 자체에는 Python 인터프리터가 필요하지 않습니다.

소스에서 빌드하려면 현재 stable Rust 도구 체인을 설치합니다.

```console
$ cargo install --path rust --locked
$ python -m venv .venv
$ .venv/bin/pip install maturin
$ .venv/bin/maturin build --release --out dist
$ .venv/bin/pip install dist/*.whl
$ .venv/bin/python -c 'import headless_reaper; print(headless_reaper.__version__)'
```

pip 소스 설치에도 Rust가 필요합니다. Python 구현만 실행하려면 체크아웃에서 `PYTHONPATH=src python -m headless_reaper`를 사용합니다. Rust crate는 edition 2021이며 직접 의존성은 Unix API용 `libc`와 JSON 출력용 `serde_json`입니다. 해석된 버전은 `Cargo.lock`에 고정합니다.

옵션, 환경 변수, 표, JSON 필드와 종료 코드는 두 구현이 같습니다. `rust/parity.sh`는 두 명령의 출력을 비교합니다. 조회 시점에 따라 달라지는 프로세스 수, 유지 시간과 RSS는 형식을 검사한 뒤 정규화합니다. 실제 테스트 프로필은 `rust/parity.sh --json --path /tmp/your-test`로 비교합니다. 정리 명령을 차례로 두 번 실행하면 첫 명령이 대상을 없애므로, `--kill` 비교는 대상이 없는 범위에서만 허용합니다. `cargo test --manifest-path rust/Cargo.toml`은 살아 있는 부모, 실제 사용자 프로필, 보조 프로세스와 경로 경계를 검사합니다.

`.github/workflows/wheels.yml`은 macOS arm64/x86_64, manylinux x86_64/aarch64, Windows x86_64 wheel과 sdist를 만듭니다. 결과물을 빌드할 뿐 PyPI에는 업로드하지 않습니다.

### 두 구현의 측정값

2026-11-17에 Rust 1.100.0 release 빌드로 측정했습니다. 시간은 프로세스 실행을 포함한 20회 순차 실행의 중앙값입니다. 시작은 `--version`, 조회는 `--json --path /tmp/headless-reaper-bench-empty`를 사용했습니다. 경로를 좁혀도 프로세스 목록 전체를 읽습니다. 최대 RSS는 별도 조회 1회에서 `/usr/bin/time`으로 측정했습니다. 파일 캐시가 준비된 상태이며, 당시 두 기기의 프로세스 구성에 대한 결과입니다.

| OS / CPU | 구현 | 시작 (ms) | 조회 (ms) | 조회 최대 RSS (KiB) | 바이너리 (bytes) |
|---|---|---:|---:|---:|---:|
| macOS 27 / arm64 | Python 3.12.8 | 36.078 | 122.098 | 23,296 | — |
| macOS 27 / arm64 | Rust | 2.856 | 11.315 | 3,136 | 620,000 |
| Ubuntu 24.04 / arm64 | Python 3.12 | 68.953 | 76.615 | 15,144 | — |
| Ubuntu 24.04 / arm64 | Rust | 0.841 | 5.564 | 1,892 | 594,688 |

정기 호출에서는 조회 시간과 RSS가 낮은 Rust 명령을 우선 사용합니다. 실행 파일이 없으면 `python -m headless_reaper`로 돌아갈 수 있습니다.

두 OS에서 `/tmp` 아래의 명시적인 일회용 프로필로 고아 브라우저를 만들었습니다. 표·JSON 비교를 통과했고, 정리 후 실행 중인 대상은 0개였으며 임시 프로필도 삭제했습니다. 살아 있는 부모가 띄운 두 번째 브라우저는 유지했습니다. macOS 대상은 10개 프로세스, Linux Chromium headless shell 대상은 6개였습니다. 18개 CLI 비교는 macOS·Linux·Windows에서 통과했습니다. Windows는 GNU x86_64 도구 체인으로 빌드하고 미지원 응답까지 실행했습니다. macOS 실행 파일은 두 번째 arm64 Mac에서도 조회했습니다.

## 쓰는 법

### 먼저 보기

```console
$ headless-reaper
```

드라이 런이라 아무 시그널도 보내지 않습니다. 머리줄에 전체 합계가 나오고, 브라우저 한 그루마다 대표 PID, 유지 시간, 메모리(RSS), 프로세스 수, 프로필 경로가 한 줄로 나옵니다. 그 아래 `why` 줄은 어떤 신호 때문에, 그리고 왜 부모가 사라졌다고 보는지 적습니다.

### 정리하기

```console
$ headless-reaper --kill
```

```console
headless-reaper 0.1.0: 1 orphaned group, 7 processes, 705.1 MB RSS (1003 processes scanned)

ROOT   AGE     RSS       PIDS  PROFILE
13251  1m 53s  705.1 MB  7     /tmp/hr-readme/profile
       why: headless, temp profile, remote debugging; launcher gone (reparented to launchd 1)

reaped 1 group: sent SIGTERM to 7 of 7 processes, SIGKILL to 0.
```

SIGTERM을 무시하는 프로세스가 있으면 같은 실행이 `SIGKILL to 1`로 끝납니다. SIGKILL까지 버티는 프로세스는 출력에 PID로 나옵니다. 종료 코드는 모두 정리했으면 0, 하나라도 남았으면 1입니다.

### 범위 좁히기

여러 작업이 같이 도는 머신이라면 두 가지로 범위를 좁힙니다.

```console
$ headless-reaper --path /srv/app/_tmp
$ headless-reaper --kill --path /srv/app/_tmp --min-age 30m
```

`--path`는 실행 파일이 아니라 프로필 디렉터리를 봅니다. 여러 개를 쓰려면 옵션을 반복합니다. `--min-age`는 그 시간보다 최근에 시작한 그루를 건너뜁니다. 방금 시작한 브라우저는 아직 준비 중인 작업의 것일 수 있기 때문입니다. 시간은 `30s`, `10m`, `2h`, `1d` 또는 초 단위 숫자로 씁니다.

### 임시 프로필까지 지우기

```console
$ headless-reaper --kill --remove-profiles
```

```console
reaped 1 group: sent SIGTERM to 7 of 7 processes, SIGKILL to 0.
removed temp profile: /tmp/hr-readme/profile
```

임시 경로 밖이거나, 살아 있는 브라우저가 아직 쓰거나, `SingletonLock`이 살아 있는 프로세스를 가리키거나, 그룹에 살아남은 프로세스가 있으면 프로필을 지우지 않고 이유를 같이 출력합니다. Playwright와 Puppeteer는 실행할 때마다 새 프로필을 만들기 때문에, 오래 쓰는 테스트 머신에서는 이 디렉터리도 함께 쌓입니다.

### 기계가 읽기

```console
$ headless-reaper --json | python3 -m json.tool
```

```json
{
    "ok": true,
    "version": "0.1.0",
    "platform": "darwin",
    "dry_run": true,
    "scanned": 1013,
    "min_age_seconds": 0,
    "paths": [
        "/tmp/hr-json"
    ],
    "totals": {
        "groups": 1,
        "processes": 13,
        "rss_bytes": 1320402944
    },
    "groups": [
        {
            "root_pid": 22308,
            "pids": [
                22308,
                22316,
                22317,
                22319,
                22322,
                22324,
                22325,
                22326,
                22328,
                22329,
                22542,
                22560,
                22596
            ],
            "processes": 13,
            "profile": "/tmp/hr-json/profile",
            "signals": [
                "headless",
                "temp profile"
            ],
            "reason": "launcher gone (reparented to launchd 1)",
            "age_seconds": 10,
            "age": "10s",
            "rss_bytes": 1320402944,
            "rss": "1.2 GB"
        }
    ],
    "reap": null
}
```

`--kill`과 함께 쓰면 `reap`이 객체가 되고 `targets`, `sigterm`, `sigkill`, `survivors`, `already_gone`, `denied`, `removed_profiles`, `kept_profiles`가 들어갑니다. 오류일 때는 `{"ok": false, "error": "…"}`를 출력하고 0이 아닌 코드로 끝납니다.

### cron이나 실행기에서 쓰기

정리할 것이 없으면 한 줄만 남기고, 정리가 끝나면 0으로 끝납니다.

```console
$ 30 4 * * * headless-reaper --kill --min-age 30m
```

정해진 시각에 도는 정리라면 `--min-age 30m`이 쓸 만한 안전장치입니다. 5분 전에 시작한 브라우저는 대개 아직 도는 작업의 것이고, 어차피 부모도 살아 있습니다.

### macOS 코드 서명 복제본

Chrome은 실행할 때 사용자의 X 임시 디렉터리에 앱 번들의 코드 서명 복제본을 만들 수 있습니다. 강제로 종료하면 이 디렉터리가 남기도 합니다. Chromium·Edge·Chrome for Testing도 같은 종류의 경로를 씁니다. 한 기기에서 복제본 174개가 겉보기 용량 193 GB를 차지했습니다. 사용하지 않는 171개를 지운 뒤 실제 여유 공간은 1.5 GB 늘었습니다. 하드 링크와 APFS 공유 블록 때문에 디렉터리 합계가 실제 회수 용량보다 클 수 있습니다. 그래도 디렉터리가 계속 쌓이고 디스크 경보를 왜곡합니다.

```console
$ headless-reaper --clean-clones                   # 드라이 런, 기본 최소 유지 시간 10분
$ headless-reaper --kill --clean-clones --min-age 30m
$ headless-reaper --all-leftovers                  # --kill + --clean-clones + --remove-profiles
```

`--clean-clones`만 쓰면 브라우저 그룹 조회나 삭제 없이 복제본만 확인합니다. 범위는 `$(getconf DARWIN_USER_TEMP_DIR)/../X` 안에서 지원하는 브라우저의 `code_sign_clone.*` 디렉터리로 고정합니다. 디렉터리 심볼릭 링크는 건너뜁니다. `--path`는 브라우저 프로필만 좁히며 복제본 범위를 바꾸지 않습니다. Linux와 Windows의 복제본 전용 조회는 `nothing to do on this OS`라고 알립니다. Windows의 `--all-leftovers`는 브라우저 백엔드가 없어 종료 코드 2를 반환합니다.

복제본은 `--min-age`보다 오래되었고 실행 중인 프로세스가 쓰지 않을 때만 삭제합니다. 두 구현 모두 실행 파일의 정확한 경로는 `proc_pidpath`, 열린 파일과 메모리 매핑은 `lsof -nP`로 읽습니다. macOS는 같은 사용자의 보호된 프로세스에도 일부 네이티브 파일 경로 조회를 거부하므로, 그 조회만으로 lsof를 대체할 수 없습니다. 경로뿐 아니라 파일의 장치·inode도 비교합니다. Chrome 복제본의 실행 파일은 `/Applications`의 실행 파일과 하드 링크로 연결될 수 있습니다. 따라서 경로만 비교하면 다음 조회에서 다른 링크 이름이 나올 수 있습니다. 실행 중인 브라우저가 여러 복제본을 함께 보존할 수 있으므로, 정기 정리는 Chrome이 종료된 다음에 실행합니다. 열린 파일 조회가 실패하면 모든 복제본을 보존하고 종료 코드 1을 반환합니다.

드라이 런은 정리 가능한 개수와 겉보기 용량을 표시합니다. JSON의 `clones`에는 `candidates`, `kept`, `removed`, `apparent_bytes`, `allocated_bytes`, `reclaimable_bytes_estimate`, `free_space_change_bytes`가 들어갑니다. 할당 블록도 공유될 수 있으므로 삭제 전 실제 회수 예상치는 `null`입니다. 삭제 후에는 파일시스템 여유 공간의 관측 변화를 표시합니다. 이 값에는 같은 시간의 다른 디스크 활동도 섞일 수 있습니다. 삭제 직전에 사용 여부를 다시 확인합니다.

로컬 검사에서는 오래된 일회용 복제본을 삭제했습니다. 복제본 밖의 하드 링크를 통해 열어 둔 파일과 X 밖 디렉터리를 가리키는 심볼릭 링크는 보존했습니다. Rust 수동 정리에서 테스트 디렉터리를 포함한 개수는 6개에서 5개가 되었습니다. 실제 Chrome 창은 종료하지 않았습니다.

## 옵션

| 옵션 | 기본값 | 뜻 |
|---|---|---|
| `--kill` | 끔 | 그룹을 정리합니다. SIGTERM, 대기, 그다음 SIGKILL |
| `--clean-clones` | 꺼짐 | macOS 코드 서명 복제본 조회, `--kill`과 함께 삭제 |
| `--all-leftovers` | 꺼짐 | `--kill --clean-clones --remove-profiles` |
| `--min-age DURATION` | `0` (복제본 정리는 `10m`) | 이 시간보다 최근에 시작한 그룹은 건너뜁니다 (`10m`, `2h`, `1d`) |
| `--path PREFIX` | 모든 경로 | `PREFIX` 아래 프로필만 봅니다. 여러 번 쓸 수 있습니다 |
| `--remove-profiles` | 끔 | `--kill`과 함께, 정리한 그룹의 임시 프로필을 지웁니다 |
| `--grace DURATION` | `10s` | SIGTERM과 SIGKILL 사이에 기다리는 시간 |
| `--json` | 끔 | 기계가 읽을 객체 하나를 출력합니다 |
| `--version` | | 버전 출력 |

종료 코드: `0`은 드라이 런이거나 모두 정리됨, `1`은 남은 프로세스가 있거나 프로세스 목록을 읽지 못함, `2`는 잘못된 사용법이거나 지원하지 않는 플랫폼입니다.

## 무엇을 대상으로 보나

아래 조건을 모두 만족할 때만 그룹의 대표 프로세스가 됩니다.

1. 실행 파일이 아는 브라우저 계열입니다. 크롬, 크로미움, 엣지, Chrome for Testing, Playwright의 크로미움과 헤드리스 셸, Puppeteer의 크롬, 파이어폭스(Playwright 빌드 포함), WebKit 미니 브라우저가 여기에 속합니다.
2. 브라우저 본체입니다. 도우미 프로세스는 제외합니다. `--type=…`이나 `-contentproc`이 있거나, 이름에 `… Helper`, `WebKitWebProcess`, `chrome_crashpad_handler`가 들어가면 도우미입니다.
3. 자동화 신호가 있습니다. `--headless`, 또는 임시 경로나 자동화 모양(`playwright_*dev_profile-*`, `puppeteer_dev_*_profile-*`, `scoped_dir*`)인 `--user-data-dir`, 또는 그런 프로필과 함께 있는 `--remote-debugging-*`이 있어야 합니다.
4. 부모가 사라졌습니다. 부모가 PID 1이거나, 이름이 `launchd`, `init`, `systemd`인 프로세스이거나, 이미 없는 PID입니다.

그 대표 프로세스 아래의 프로세스 전체가 한 그룹입니다. 대표가 죽은 뒤 init으로 옮겨 간 도우미 프로세스는 같은 프로필 경로를 통해 그룹에 합쳐지고, 따로 떨어진 그룹이 되지 않습니다.

대상이 되지 않는 것:

- macOS나 Linux의 실제 사용자 프로필(`~/Library/Application Support/Google/Chrome`, `~/.config/google-chrome`, `~/.mozilla/firefox` 등)을 쓰는 브라우저. `--headless`가 붙어 있어도 대상이 아닙니다.
- 자동화용 프로필이 없는 화면 있는 브라우저. `--remote-debugging-port`가 있어도 대상이 아닙니다.
- 부모 프로세스가 살아 있는 브라우저.

## 환경 변수

| 변수 | 같은 옵션 | 예 |
|---|---|---|
| `HEADLESS_REAPER_MIN_AGE` | `--min-age` | `30m` |
| `HEADLESS_REAPER_PATH` | `--path` | `/srv/app/_tmp:/srv/other/_tmp` |
| `HEADLESS_REAPER_GRACE` | `--grace` | `5s` |

명령줄 옵션이 항상 환경 변수보다 우선합니다. `HEADLESS_REAPER_PATH`는 macOS와 Linux에서 `:`로 나눕니다.

## 파이썬에서 쓰기

```python
import time

from headless_reaper import clean_clones, cleanup_profiles, find_groups, list_processes, reap

procs = list_processes()
groups = find_groups(procs, now=time.time(), min_age=600, paths=("/srv/app/_tmp",))
for group in groups:
    print(group.root.pid, group.profile, group.signals, group.rss_bytes())

result = reap(groups)
print(result.sigterm, result.sigkill, result.survivors)

removed, kept = cleanup_profiles(groups)
```

`list_processes()`는 `Proc` 레코드(`pid`, `ppid`, `rss_bytes`, `started_at`, `state`, `exe`, `argv`)를 돌려줍니다. `find_groups()`는 `root`, `members`, `profile`, `signals`, `reason`이 담긴 `Group`을 돌려주고, `pids`, `rss_bytes()`, `age_seconds(now)`를 함께 제공합니다.

## 한계

시험한 환경:

- macOS 27, arm64, Google Chrome 157. 짧게 살다 끝나는 부모에서 헤드리스 크롬을 띄워 일부러 방치한 경우와, 부모가 살아 있는 채로 띄운 경우를 각각 만들었습니다. 첫 번째는 찾아서 정리했고, 두 번째는 그대로 두었으며, 이 머신에서 평소 쓰는 크롬 창은 건드리지 않았습니다.
- Ubuntu 24.04, arm64, Playwright Chromium 헤드리스 셸 159와 Playwright Firefox 159. 엔진마다 방치 프로세스를 하나씩 만들어 찾고, 정리하고, 임시 프로필까지 지웠습니다. Playwright가 실제로 넘기는 인자(크로미움은 `--headless`, `--user-data-dir=/tmp/playwright_chromiumdev_profile-…`, `--remote-debugging-pipe`, 파이어폭스는 `-headless -profile …`)가 이 도구가 맞추는 값과 같습니다.

시험하지 않은 것:

- **Windows 브라우저 탐지.** 두 구현 모두 백엔드가 없습니다. Rust GNU x86_64 빌드와 종료 코드 2 응답까지 확인했으며, 브라우저는 조회하거나 정리하지 않습니다.
- **WebKit 실제 실행.** 시험한 머신의 Playwright WebKit 빌드에 GTK 4 라이브러리가 없어서, 인자 모양(`pw_run.sh --headless --user-data-dir=/tmp/playwright_webkitdev_profile-…`)만 탐지 규칙에 넣어 확인했고 실제 브라우저는 띄우지 못했습니다.
- **macOS/Linux x86_64.** 두 OS의 프로세스 백엔드는 x86_64에서 실행하지 않았습니다. Windows x86_64에서는 미지원 플랫폼 분기만 실행했습니다.
- **Python macOS `ps`와 공백.** Python 참조 구현은 `ps`의 공백 구분 출력에서 공백이 들어간 프로필 경로를 복원하지 못합니다. Rust는 `KERN_PROCARGS2`로 정확한 인자를 읽고 libproc으로 PID·부모·시작 시각·RSS를 읽습니다. Linux에서는 두 구현 모두 정확한 인자를 읽습니다. macOS에서 조회할 수 없는 PID도 목록에 유지하므로, 그 자식의 살아 있는 부모를 사라졌다고 판단하지 않습니다.
- **launchd나 systemd가 직접 띄운 브라우저.** 부모가 init 프로세스라서 방치된 것과 똑같이 보입니다. `--path`로 범위를 좁히거나, 재시작 주기보다 긴 `--min-age`를 주세요.
- **실제 프로필을 쓰는 화면 있는 자동화 브라우저.** 예를 들어 `headless: false`로 띄운 Puppeteer가 홈 디렉터리 아래 프로젝트 프로필을 쓰는 경우, 부모가 죽어도 정리하지 않습니다. 사람이 쓰고 있을 수 있는 창을 닫느니 그대로 두는 쪽을 고른 것입니다. 이런 프로필도 정리하고 싶다면 임시 경로 아래에 두세요.

## 에이전트에서 쓰기

AI 에이전트가 남은 브라우저를 정리해야 한다면 [AGENTS.md](https://github.com/elfenomenokim/headless-reaper/blob/main/AGENTS.md)를 읽으세요. Claude Code 스킬은 [`skill/headless-reaper/SKILL.md`](https://github.com/elfenomenokim/headless-reaper/blob/main/skill/headless-reaper/SKILL.md)에 있습니다. 이 폴더를 `~/.claude/skills/headless-reaper/`로 복사하면 설치됩니다.

## 라이선스

MIT입니다. [LICENSE](https://github.com/elfenomenokim/headless-reaper/blob/main/LICENSE)를 보세요.
