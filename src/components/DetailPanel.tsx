import { useState } from 'react';
import type { AnalysisResult } from './AnalysisTable';

interface Props {
  result: AnalysisResult | null;
}

export default function DetailPanel({ result }: Props) {
  const [refsOpen, setRefsOpen] = useState(true);
  const [debugLogOpen, setDebugLogOpen] = useState(false);

  if (!result) {
    return (
      <div className="detail-panel" style={{ justifyContent: 'center' }}>
        <div className="empty-state">
          <div className="empty-icon">👈</div>
          <p>행을 클릭하면 세부 정보를 볼 수 있습니다.</p>
        </div>
      </div>
    );
  }

  const Row = ({ label, value }: { label: string; value: React.ReactNode }) => (
    <div className="detail-section">
      <div className="detail-label">{label}</div>
      <div className="detail-value">{value || <span style={{ color: 'var(--text-muted)' }}>—</span>}</div>
    </div>
  );

  const statusLabel: Record<string, string> = {
    Found: '✓ 발견',
    NotFound: '✗ 미발견',
    ManualCheck: '⚠ 수동확인',
    Error: '! 오류',
  };

  return (
    <div className="detail-panel fade-in" style={{ overflowY: 'auto' }}>
      <div style={{ padding: '12px 18px', borderBottom: '1px solid var(--border)', fontWeight: 600, fontSize: 13 }}>
        세부 정보
      </div>

      <Row label="분석 ID" value={result.result_id} />
      <Row label="유형" value={result.analysis_type === 'combo' ? '콤보 (getComCodeCombo 등)' : 'actionSubmit(FR)'} />
      <Row label="상태" value={statusLabel[result.status]} />
      <Row label="XFDL 파일" value={result.xfdl_name} />
      {result.analysis_type === 'combo' ? (
        <>
          <Row label="콤보 파라미터" value={result.combo_param ?? '—'} />
          <Row label="공통코드 사용" value={result.is_common_code === true ? '예' : result.is_common_code === false ? '호출 쿼리' : '—'} />
        </>
      ) : (
        <Row label="서비스 URL" value={result.service_url} />
      )}
      <Row label="Java 파일" value={result.java_file} />
      <Row label="클래스 파일" value={result.class_file} />
      <Row
        label="메서드"
        value={
          result.method_name
            ? `${result.method_name}() — Line ${result.method_line ?? '?'}`
            : null
        }
      />

      {result.error_msg && (
        <div className="detail-section">
          <div className="detail-label" style={{ color: 'var(--error)' }}>오류 메시지</div>
          <div className="detail-value" style={{ color: 'var(--error)', fontSize: 12 }}>
            {result.error_msg}
          </div>
        </div>
      )}

      {/* 나중에 쉽게 지울 수 있는 디버그 로그 패널 영역 (TEST 용) */}
      <div className="detail-section" style={{ background: 'rgba(236, 239, 244, 0.4)' }}>
        <button
          style={{
            display: 'flex', alignItems: 'center', gap: 6,
            background: 'none', border: 'none', cursor: 'pointer',
            padding: 0, width: '100%', textAlign: 'left',
          }}
          onClick={() => setDebugLogOpen((o) => !o)}
        >
          <span style={{ transform: debugLogOpen ? 'rotate(90deg)' : 'rotate(0deg)', transition: '150ms', display: 'inline-block', fontSize: 10, color: 'var(--text-muted)' }}>▶</span>
          <div className="detail-label" style={{ margin: 0, display: 'flex', alignItems: 'center', gap: 6 }}>
            디버그 로그 (테스트용) <span style={{ background: 'var(--nord10)', color: 'white', padding: '2px 6px', borderRadius: 4, fontSize: 9 }}>TEST</span>
          </div>
        </button>
        {debugLogOpen && (
          <div style={{ marginTop: 8, padding: '12px', background: '#f5f5f5', border: '1px solid #ddd', borderRadius: 4, maxHeight: 200, overflowY: 'auto', fontSize: 11, fontFamily: 'JetBrains Mono, monospace', whiteSpace: 'pre-wrap', color: 'var(--nord0)' }}>
            {result.debug_logs && result.debug_logs.length > 0 ? result.debug_logs.join('\n') : '로그 없음'}
          </div>
        )}
      </div>

      {/* 쿼리 목록 */}
      <div className="detail-section">
        <button
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 6,
            background: 'none',
            border: 'none',
            cursor: 'pointer',
            padding: 0,
            width: '100%',
            textAlign: 'left',
          }}
          onClick={() => setRefsOpen((o) => !o)}
        >
          <span
            style={{
              transform: refsOpen ? 'rotate(90deg)' : 'rotate(0deg)',
              transition: '150ms',
              display: 'inline-block',
              fontSize: 10,
              color: 'var(--text-muted)',
            }}
          >
            ▶
          </span>
          <div className="detail-label" style={{ margin: 0 }}>
            호출 쿼리 ({result.queries?.length || 0}건)
          </div>
        </button>

        {refsOpen && (
          <div className="ref-list" style={{ marginTop: 8 }}>
            {!result.queries || result.queries.length === 0 ? (
              <span style={{ color: 'var(--text-muted)', fontSize: 12 }}>쿼리 없음</span>
            ) : (
              result.queries.map((q, i) => (
                <div key={i} className="ref-item" style={{ marginBottom: '8px', paddingBottom: '8px', borderBottom: '1px solid var(--border)' }}>
                  <div style={{ fontWeight: 'bold', color: 'var(--nord10)', marginBottom: '4px' }}>{q.query_id}</div>
                  <div style={{ fontSize: '11px', color: 'var(--text-muted)' }}>{q.call_path.join(' ➔ ')}</div>
                </div>
              ))
            )}
          </div>
        )}
      </div>
    </div>
  );
}
