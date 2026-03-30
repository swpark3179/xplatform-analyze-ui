# XFDL Service Analyzer

**XFDL Service Analyzer**는 XFDL(XML Forms Definition Language) 파일에서 서비스 호출(`actionSubmit`, `getComCodeCombo` 등)을 분석하고, 매핑되는 Java 소스 및 컴파일된 `.class` 파일에서 쿼리 ID 호출 흐름을 추출하여 Excel 형태로 저장할 수 있게 해주는 참조관계 분석 데스크탑 애플리케이션입니다.

![App Screenshot](public/screenshot.png) <!-- 스크린샷이 있다면 추가하세요 -->

## 프로젝트 개요

이 애플리케이션은 **React** 기반의 Nord Light 테마 UI와 **Rust** 기반의 빠르고 강력한 파일 분석 엔진(Tauri)으로 구성되어 있습니다.

지정된 프로젝트 Root 하위의 `src/webapp/ui` 경로에 존재하는 모든 `.xfdl` 파일을 스캔하여, JavaScript(`<![CDATA[ ... ]]>`) 블록 내에서 실행되는 서비스 호출과 관련된 Java 파일 및 실제 실행되는 쿼리를 추적하여 화면에 가상 테이블 형태로 표시하고 엑셀 파일로 출력하는 기능을 제공합니다.

## 주요 기능

1. **XFDL 파일 스캔 및 선택**
   - 프로젝트 Root 경로를 선택하면 하위의 모든 `.xfdl` 파일을 스캔하여 목록을 제공합니다.
   - 검색 및 필터링을 통해 분석할 대상을 선택할 수 있습니다.
2. **서비스 호출(Action) 자동 분석**
   - 선택된 XFDL 파일의 `Script` 태그 내에서 `actionSubmit`, `actionSubmitFR` 등의 호출 내역을 탐색합니다.
   - XFDL 파일 내 `<Dataset id="dsAction">`을 분석하여, 각 Action ID에 매핑되는 실제 서비스 URL을 찾습니다.
   - `default_typedef.xml`을 파싱하여 서비스 Prefix를 추출하고, 이를 기반으로 매핑되는 Java Controller/Service 클래스와 메서드를 찾습니다.
3. **콤보/공통코드(Combo) 추출**
   - `getComCodeCombo`, `getGridCodeCombo` 계열 함수의 호출 내역을 추출합니다.
   - 파라미터가 9자 이하면 '공통코드', 초과하면 '호출 쿼리'로 자동 분류합니다.
4. **Java Bytecode 및 AST 분석 (바이너리 분석)**
   - `target/classes` 디렉토리 하위의 `.class` 파일을 읽어(Bytecode), 메서드가 호출하는 내부 쿼리(Query ID) 참조 경로를 분석합니다.
5. **결과 시각화 및 엑셀(Excel) 저장**
   - 분석 결과(발견, 미발견, 수동확인, 오류) 및 호출된 쿼리 목록을 가상 테이블 UI를 통해 직관적으로 보여줍니다.
   - 분석된 내역을 `.xlsx` 형식의 엑셀 파일로 저장할 수 있습니다.

## 기술 스택

- **Frontend (UI)**: React 19, TypeScript, Vite, `@tanstack/react-table`, `@tanstack/react-virtual`
- **Backend (Core Engine)**: Rust, Tauri v2
- **XML Parsing**: `quick-xml`
- **Java Classfile Parsing**: `cafebabe` (바이너리 분석용)
- **Excel Export**: `rust_xlsxwriter` (Rust), `xlsx` (React)

## 설치 및 실행 방법

### 사전 요구사항
이 프로젝트를 빌드하고 실행하기 위해서는 다음 환경이 준비되어 있어야 합니다.
- **Node.js** (v18 이상 권장)
- **Rust / Cargo** (최신 안정화 버전)
- **C/C++ Build Tools** (Tauri 빌드용. Windows의 경우 Visual Studio C++ Build Tools)

### 1. 패키지 설치
```bash
npm install
```

### 2. 개발 모드 실행
UI와 Rust 백엔드를 동시에 개발 모드로 실행합니다.
```bash
npm run tauri dev
```

### 3. 애플리케이션 빌드
배포용 실행 파일(Windows의 경우 `.exe`, macOS의 경우 `.app`)을 생성합니다.
```bash
npm run tauri build
```
빌드된 파일은 `src-tauri/target/release/bundle` 경로에 생성됩니다.

## 디렉토리 구조 설명
- `/src`: React 기반의 프론트엔드 UI 소스 코드가 포함되어 있습니다.
- `/src-tauri`: Rust 기반의 Tauri 백엔드 로직이 포함되어 있습니다.
  - `/src-tauri/src/commands`: 파일 스캔(`scan.rs`), 참조 분석(`analyze.rs`), 엑셀 내보내기(`export.rs`) 등의 주요 기능(Command)이 구현되어 있습니다.
  - `/src-tauri/src/parser`: XML 파싱 및 Java Bytecode(`cafebabe`) 분석 로직이 존재합니다.
- `/src-tauri/tauri.conf.json`: Tauri 프로젝트 설정 파일입니다.
- `/public`: 정적 에셋 파일들이 위치합니다.
- `/spec.md`: 초기 분석 스펙 문서 (참고용)

## 동작 원리 요약
1. 프론트엔드에서 **Root 경로**를 Tauri 백엔드로 전달합니다.
2. 백엔드의 `scan_xfdl_files` 명령이 실행되어 XFDL 파일 목록을 수집하여 반환합니다.
3. 분석 시작 시, `analyze_actions` 명령이 실행되며 각 XFDL 파일을 읽고, 정규식을 통해 `actionSubmit` 또는 `Combo` 호출 위치를 찾습니다.
4. XML 파서를 통해 서비스 URL을 추출한 후, 이를 바탕으로 `default_typedef.xml`의 prefix를 조합하여 Java 소스파일 및 `.class` 바이너리의 경로를 추적합니다.
5. `cafebabe` 라이브러리로 컴파일된 Java 클래스(Bytecode)를 분석하여 해당 메서드 내에서 호출되는 쿼리 사용 내역을 최종 결과에 담아 프론트엔드에 전달합니다.
