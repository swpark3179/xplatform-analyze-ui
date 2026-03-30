import { useState, useMemo } from 'react';

interface XfdlFile {
  path: string;
  name: string;
}

interface ScanProgress {
  current: number;
  total: number;
  current_file: string;
}

interface Props {
  files: XfdlFile[];
  selected: Set<string>;
  onToggle: (path: string) => void;
  onToggleAll: (paths: string[]) => void;
  scanProgress: ScanProgress | null;
  isScanning: boolean;
}

export default function FileList({
  files,
  selected,
  onToggle,
  onToggleAll,
  scanProgress,
  isScanning,
}: Props) {
  const [query, setQuery] = useState('');

  const filtered = useMemo(() => {
    if (!query.trim()) return files;
    const q = query.toLowerCase();
    return files.filter(
      (f) =>
        f.name.toLowerCase().includes(q) ||
        f.path.toLowerCase().includes(q)
    );
  }, [files, query]);

  const allFilteredSelected =
    filtered.length > 0 && filtered.every((f) => selected.has(f.path));

  const handleToggleAll = () => {
    onToggleAll(filtered.map((f) => f.path));
  };

  const pct = scanProgress
    ? Math.round((scanProgress.current / scanProgress.total) * 100)
    : 0;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: 10 }}>
      {/* 스캔 진행 */}
      {isScanning && scanProgress && (
        <div className="fade-in">
          <div className="progress-bar-wrap">
            <div className="progress-bar-fill" style={{ width: `${pct}%` }} />
          </div>
          <div className="progress-label">
            {scanProgress.current_file} ({scanProgress.current} / {scanProgress.total})
          </div>
        </div>
      )}

      {/* 툴바 */}
      <div className="file-list-toolbar">
        <div className="input-wrap" style={{ flex: 1 }}>
          <svg className="input-icon" width="14" height="14" viewBox="0 0 24 24"
            fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          <input
            type="text"
            placeholder="파일명으로 검색..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <span className="file-count-badge">
          {selected.size} / {files.length} 선택
        </span>
        <button
          className="btn btn-sm btn-secondary"
          onClick={handleToggleAll}
          disabled={filtered.length === 0}
        >
          {allFilteredSelected ? '전체 해제' : '전체 선택'}
        </button>
      </div>

      {/* 목록 */}
      <div className="file-list-scroll">
        {files.length === 0 && !isScanning ? (
          <div className="empty-state">
            <div className="empty-icon">📂</div>
            <p>스캔된 XFDL 파일이 없습니다.</p>
          </div>
        ) : filtered.length === 0 ? (
          <div className="empty-state">
            <div className="empty-icon">🔍</div>
            <p>검색 결과가 없습니다.</p>
          </div>
        ) : (
          filtered.map((file) => {
            const isChecked = selected.has(file.path);
            return (
              <div
                key={file.path}
                className={`file-list-item ${isChecked ? 'selected' : ''}`}
                onClick={() => onToggle(file.path)}
              >
                <input
                  type="checkbox"
                  checked={isChecked}
                  onChange={() => onToggle(file.path)}
                  onClick={(e) => e.stopPropagation()}
                />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="file-name">{file.name}.xfdl</div>
                  <div className="file-path">{file.path}</div>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
