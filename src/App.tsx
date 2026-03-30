import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { save } from '@tauri-apps/plugin-dialog';
import './App.css';

import FolderSelector from './components/FolderSelector';
import FileList from './components/FileList';
import AnalysisTable, { type AnalysisResult } from './components/AnalysisTable';
import DetailPanel from './components/DetailPanel';

// ─── Types ───────────────────────────────────
interface XfdlFile {
  path: string;
  name: string;
}

interface ScanProgress {
  current: number;
  total: number;
  current_file: string;
}

interface AnalyzeProgress {
  current: number;
  total: number;
  current_id: string;
  status: string;
}

type Phase = 'select' | 'scan' | 'choose' | 'analyze' | 'result';

// ─── Step indicator ───────────────────────────
const STEPS: { key: Phase; label: string }[] = [
  { key: 'select', label: '폴더 선택' },
  { key: 'choose', label: '파일 선택' },
  { key: 'analyze', label: '분석 중' },
  { key: 'result', label: '결과 보기' },
];

const PHASE_ORDER: Phase[] = ['select', 'scan', 'choose', 'analyze', 'result'];

function StepIndicator({ phase }: { phase: Phase }) {
  const current = PHASE_ORDER.indexOf(phase);
  return (
    <div className="stepper">
      {STEPS.map((step, i) => {
        const idx = PHASE_ORDER.indexOf(step.key);
        const isDone = current > idx;
        const isActive = current === idx || (step.key === 'choose' && phase === 'scan');
        return (
          <div key={step.key} style={{ display: 'flex', alignItems: 'center' }}>
            {i > 0 && <div className="step-divider" />}
            <div className={`step ${isDone ? 'done' : isActive ? 'active' : ''}`}>
              <div className="step-num">{isDone ? '✓' : i + 1}</div>
              {step.label}
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ─── App ─────────────────────────────────────
export default function App() {
  const [phase, setPhase] = useState<Phase>('select');
  const [rootPath, setRootPath] = useState('');
  const [xfdlFiles, setXfdlFiles] = useState<XfdlFile[]>([]);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [scanProgress, setScanProgress] = useState<ScanProgress | null>(null);
  const [isScanning, setIsScanning] = useState(false);
  const [analyzeProgress, setAnalyzeProgress] = useState<AnalyzeProgress | null>(null);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [results, setResults] = useState<AnalysisResult[]>([]);
  const [selectedResult, setSelectedResult] = useState<AnalysisResult | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  // 필터: 발견+오류없음+쿼리0건 숨기기 / 분석 유형 / 공통코드
  const [filterHideFoundNoQueries, setFilterHideFoundNoQueries] = useState(false);
  const [filterAnalysisType, setFilterAnalysisType] = useState<'all' | 'actionSubmit' | 'combo'>('all');
  const [filterCommonCode, setFilterCommonCode] = useState<'all' | 'include' | 'exclude' | 'only'>('all');

  // ─── Scan ───────────────────────────────────
  const handleScan = useCallback(async () => {
    if (!rootPath) return;
    setErrorMsg(null);
    setIsScanning(true);
    setPhase('scan');
    setXfdlFiles([]);
    setScanProgress(null);

    const unlisten = await listen<ScanProgress>('scan_progress', (e) => {
      setScanProgress(e.payload);
    });

    try {
      const files = await invoke<XfdlFile[]>('scan_xfdl_files', { rootPath });
      setXfdlFiles(files);
      setSelectedPaths(new Set(files.map((f) => f.path)));
      setPhase('choose');
    } catch (e) {
      setErrorMsg(String(e));
      setPhase('select');
    } finally {
      setIsScanning(false);
      unlisten();
    }
  }, [rootPath]);

  // ─── Toggle selection ────────────────────────
  const handleToggle = (path: string) => {
    setSelectedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const handleToggleAll = (paths: string[]) => {
    const allSelected = paths.every((p) => selectedPaths.has(p));
    setSelectedPaths((prev) => {
      const next = new Set(prev);
      if (allSelected) paths.forEach((p) => next.delete(p));
      else paths.forEach((p) => next.add(p));
      return next;
    });
  };

  // ─── Analyze ────────────────────────────────
  const handleAnalyze = useCallback(async () => {
    if (selectedPaths.size === 0) return;
    setErrorMsg(null);
    setIsAnalyzing(true);
    setPhase('analyze');
    setResults([]);
    setSelectedResult(null);

    const unlisten = await listen<AnalyzeProgress>('analyze_progress', (e) => {
      setAnalyzeProgress(e.payload);
    });

    try {
      const data = await invoke<AnalysisResult[]>('analyze_actions', {
        rootPath,
        xfdlPaths: Array.from(selectedPaths),
      });
      setResults(data);
      setPhase('result');
    } catch (e) {
      setErrorMsg(String(e));
      setPhase('choose');
    } finally {
      setIsAnalyzing(false);
      unlisten();
    }
  }, [rootPath, selectedPaths]);



  // 필터된 결과 (테이블·엑셀 저장에 사용)
  const filteredResults = ((): AnalysisResult[] => {
    let list = results;
    if (filterHideFoundNoQueries) {
      list = list.filter(
        (r) =>
          !(
            r.status === 'Found' &&
            !r.error_msg &&
            (r.queries?.length ?? 0) === 0
          )
      );
    }
    if (filterAnalysisType === 'actionSubmit') {
      list = list.filter((r) => r.analysis_type !== 'combo');
    } else if (filterAnalysisType === 'combo') {
      list = list.filter((r) => r.analysis_type === 'combo');
    }
    if (filterCommonCode === 'exclude') {
      list = list.filter((r) => r.is_common_code !== true);
    } else if (filterCommonCode === 'only') {
      list = list.filter((r) => r.is_common_code === true);
    }
    return list;
  })();

  // 필터로 인해 선택 항목이 목록에 없으면 선택 해제
  useEffect(() => {
    if (selectedResult && !filteredResults.some((r) => r.result_id === selectedResult.result_id)) {
      setSelectedResult(null);
    }
  }, [filteredResults, selectedResult]);

  // ─── Export (필터된 결과만 저장) ─────────────────────────────────
  const handleExportFiltered = useCallback(async () => {
    const savePath = await save({
      filters: [{ name: 'Excel', extensions: ['xlsx'] }],
      defaultPath: 'analysis_result.xlsx',
    });
    if (!savePath) return;
    try {
      await invoke('export_excel', { results: filteredResults, savePath });
      alert('엑셀 파일이 저장되었습니다.');
    } catch (e) {
      alert('저장 실패: ' + e);
    }
  }, [filteredResults]);

  // ─── Render ─────────────────────────────────
  const analyzePct =
    analyzeProgress && analyzeProgress.total > 0
      ? Math.round((analyzeProgress.current / analyzeProgress.total) * 100)
      : 0;

  return (
    <div className="app-layout">
      {/* Header */}
      <header className="app-header">
        <div className="logo">A</div>
        <h1>XFDL Service Analyzer</h1>
        <div style={{ flex: 1 }} />
        <StepIndicator phase={phase} />
      </header>

      {/* Content */}
      <div className="app-content">
        {/* 오류 메시지 */}
        {errorMsg && (
          <div
            style={{
              padding: '10px 16px',
              background: 'rgba(191,97,106,0.1)',
              border: '1px solid var(--error)',
              borderRadius: 'var(--radius-sm)',
              color: 'var(--error)',
              fontSize: 13,
              display: 'flex',
              alignItems: 'center',
              gap: 8,
            }}
          >
            ⚠ {errorMsg}
            <button
              onClick={() => setErrorMsg(null)}
              style={{ marginLeft: 'auto', background: 'none', border: 'none', cursor: 'pointer', color: 'var(--error)' }}
            >
              ✕
            </button>
          </div>
        )}

        {/* ── Phase: select / scan / choose ── */}
        {(phase === 'select' || phase === 'scan' || phase === 'choose') && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 16, flex: 1, minHeight: 0 }}>
            {/* 폴더 선택 */}
            <div className="card">
              <div className="card-header">📁 프로젝트 루트 폴더</div>
              <div className="card-body" style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
                <FolderSelector
                  rootPath={rootPath}
                  onChange={(p) => { setRootPath(p); setPhase('select'); setXfdlFiles([]); }}
                  disabled={isScanning || isAnalyzing}
                />
                <button
                  className="btn btn-primary"
                  disabled={!rootPath || isScanning}
                  onClick={handleScan}
                >
                  {isScanning ? (
                    <>
                      <span className="spin" style={{ display: 'inline-block' }}>⟳</span>
                      스캔 중...
                    </>
                  ) : (
                    '🔍 XFDL 스캔'
                  )}
                </button>
              </div>
            </div>

            {/* 파일 목록 */}
            {(phase === 'scan' || phase === 'choose') && (
              <div className="card fade-in" style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
                <div className="card-header">
                  📄 XFDL 파일 목록
                  <div style={{ display: 'flex', gap: 8 }}>
                    <span className="file-count-badge">{xfdlFiles.length}개 파일</span>
                    <button
                      className="btn btn-sm btn-primary"
                      disabled={selectedPaths.size === 0 || isScanning || isAnalyzing}
                      onClick={handleAnalyze}
                    >
                      ▶ 분석 시작 ({selectedPaths.size}개)
                    </button>
                  </div>
                </div>
                <div className="card-body" style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
                  <FileList
                    files={xfdlFiles}
                    selected={selectedPaths}
                    onToggle={handleToggle}
                    onToggleAll={handleToggleAll}
                    scanProgress={scanProgress}
                    isScanning={isScanning}
                  />
                </div>
              </div>
            )}
          </div>
        )}

        {/* ── Phase: analyze ── */}
        {phase === 'analyze' && (
          <div
            style={{
              flex: 1,
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'center',
              gap: 24,
            }}
          >
            <div style={{ fontSize: 40 }}>🔬</div>
            <div style={{ fontWeight: 600, fontSize: 16 }}>분석 중...</div>
            {analyzeProgress && (
              <div style={{ width: 400, display: 'flex', flexDirection: 'column', gap: 8 }}>
                <div style={{ fontSize: 12, color: 'var(--text-secondary)' }}>
                  {analyzeProgress.status}
                </div>
                {analyzeProgress.total > 0 && (
                  <>
                    <div className="progress-bar-wrap" style={{ height: 8 }}>
                      <div className="progress-bar-fill" style={{ width: `${analyzePct}%` }} />
                    </div>
                    <div className="progress-label">
                      {analyzeProgress.current_id} ({analyzeProgress.current} / {analyzeProgress.total})
                    </div>
                  </>
                )}
              </div>
            )}
          </div>
        )}

        {/* ── Phase: result ── */}
        {phase === 'result' && (
          <>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' }}>
              <button
                className="btn btn-secondary btn-sm"
                onClick={() => { setPhase('choose'); setResults([]); setSelectedResult(null); }}
              >
                ← 다시 선택
              </button>
              <span style={{ fontSize: 13, color: 'var(--text-secondary)' }}>
                총 <strong>{results.length}</strong>건 (필터 후 <strong>{filteredResults.length}</strong>건)
              </span>
              <div style={{ flex: 1, minWidth: 0 }} />
              <button className="btn btn-success btn-sm" onClick={handleExportFiltered}>
                💾 엑셀 저장
              </button>
            </div>

            {/* 필터 */}
            <div
              style={{
                display: 'flex',
                flexWrap: 'wrap',
                alignItems: 'center',
                gap: 12,
                padding: '10px 14px',
                background: 'var(--bg-subtle)',
                borderRadius: 'var(--radius-sm)',
                fontSize: 13,
              }}
            >
              <label style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                <input
                  type="checkbox"
                  checked={filterHideFoundNoQueries}
                  onChange={(e) => setFilterHideFoundNoQueries(e.target.checked)}
                />
                발견·오류없음·쿼리 0건 숨기기
              </label>
              <span style={{ color: 'var(--text-muted)' }}>|</span>
              <label>
                분석 유형{' '}
                <select
                  value={filterAnalysisType}
                  onChange={(e) => setFilterAnalysisType(e.target.value as 'all' | 'actionSubmit' | 'combo')}
                  style={{ marginLeft: 4, padding: '4px 8px' }}
                >
                  <option value="all">전체</option>
                  <option value="actionSubmit">actionSubmit(FR)만</option>
                  <option value="combo">콤보만</option>
                </select>
              </label>
              <span style={{ color: 'var(--text-muted)' }}>|</span>
              <label>
                공통코드{' '}
                <select
                  value={filterCommonCode}
                  onChange={(e) => setFilterCommonCode(e.target.value as 'all' | 'include' | 'exclude' | 'only')}
                  style={{ marginLeft: 4, padding: '4px 8px' }}
                >
                  <option value="all">전체</option>
                  <option value="exclude">제외</option>
                  <option value="only">공통코드만</option>
                </select>
              </label>
            </div>

            <div className="result-pane">
              <div className="table-pane">
                <div className="card" style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
                  <div className="card-header">
                    분석 결과
                    <div style={{ display: 'flex', gap: 6, fontSize: 12 }}>
                      {(['Found', 'NotFound', 'ManualCheck', 'Error'] as const).map((s) => {
                        const count = filteredResults.filter((r) => r.status === s).length;
                        const cls = `badge badge-${s.toLowerCase()}`;
                        const lbl: Record<string, string> = {
                          Found: '발견',
                          NotFound: '미발견',
                          ManualCheck: '수동확인',
                          Error: '오류',
                        };
                        return (
                          <span key={s} className={cls}>
                            {lbl[s]} {count}
                          </span>
                        );
                      })}
                    </div>
                  </div>
                  <div style={{ flex: 1, minHeight: 0, overflow: 'hidden' }}>
                    <AnalysisTable
                      results={filteredResults}
                      selectedId={selectedResult?.result_id ?? null}
                      onSelect={setSelectedResult}
                    />
                  </div>
                </div>
              </div>

              <div className="detail-pane">
                <DetailPanel result={selectedResult} />
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
