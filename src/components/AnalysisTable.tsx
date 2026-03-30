import { useRef } from 'react';
import {
  useReactTable,
  getCoreRowModel,
  flexRender,
  createColumnHelper,
} from '@tanstack/react-table';
import { useVirtualizer } from '@tanstack/react-virtual';

export type AnalysisStatus = 'Found' | 'NotFound' | 'ManualCheck' | 'Error';

export interface QueryUsage {
  call_path: string[];
  query_id: string;
}

export type AnalysisType = 'actionSubmit' | 'combo';

export interface AnalysisResult {
  result_id: string;
  action_id: string;
  xfdl_path: string;
  xfdl_name: string;
  service_url: string | null;
  status: AnalysisStatus;
  java_file: string | null;
  class_file: string | null;
  method_name: string | null;
  method_line: number | null;
  queries: QueryUsage[];
  error_msg: string | null;
  debug_logs: string[];
  analysis_type?: AnalysisType;
  combo_param?: string | null;
  is_common_code?: boolean | null;
}

interface Props {
  results: AnalysisResult[];
  selectedId: string | null;
  onSelect: (result: AnalysisResult) => void;
}

const statusBadge = (s: AnalysisStatus) => {
  const map: Record<AnalysisStatus, [string, string]> = {
    Found: ['badge badge-found', '✓ 발견'],
    NotFound: ['badge badge-notfound', '✗ 미발견'],
    ManualCheck: ['badge badge-manual', '⚠ 수동확인'],
    Error: ['badge badge-error', '! 오류'],
  };
  const [cls, label] = map[s];
  return <span className={cls}>{label}</span>;
};

const col = createColumnHelper<AnalysisResult>();

const columns = [
  col.accessor('result_id', {
    header: '분석 ID',
    cell: (i) => (
      <span style={{ fontFamily: 'JetBrains Mono, monospace', fontSize: 12 }}>
        {i.getValue()}
      </span>
    ),
    size: 240,
  }),
  col.accessor('analysis_type', {
    header: '유형',
    cell: (i) => {
      const v = i.getValue();
      if (v === 'combo') return <span className="badge badge-manual">콤보</span>;
      return <span className="td-mono">actionSubmit</span>;
    },
    size: 90,
  }),
  col.accessor('status', {
    header: '상태',
    cell: (i) => statusBadge(i.getValue()),
    size: 100,
  }),
  col.accessor((r) => r.combo_param ?? r.service_url, {
    id: 'combo_or_url',
    header: '콤보 파라미터 / 서비스 URL',
    cell: (i) => (
      <span className="td-mono">
        {i.row.original.combo_param ?? i.row.original.service_url ?? '—'}
      </span>
    ),
    size: 220,
  }),
  col.accessor('java_file', {
    header: 'Java 파일',
    cell: (i) => {
      const val = i.getValue();
      if (!val) return <span style={{ color: 'var(--text-muted)' }}>—</span>;
      const parts = val.replace(/\\/g, '/').split('/');
      return <span className="td-mono">{parts[parts.length - 1]}</span>;
    },
    size: 180,
  }),
  col.accessor('method_name', {
    header: '메서드',
    cell: (i) => (
      <span className="td-mono">{i.getValue() ?? '—'}</span>
    ),
    size: 160,
  }),
  col.accessor('method_line', {
    header: '라인',
    cell: (i) => (
      <span className="td-mono">{i.getValue() ?? '—'}</span>
    ),
    size: 70,
  }),
];

const ROW_HEIGHT = 40;

export default function AnalysisTable({ results, selectedId, onSelect }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const table = useReactTable({
    data: results,
    columns,
    getCoreRowModel: getCoreRowModel(),
  });
  const rows = table.getRowModel().rows;

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });
  const virtualRows = rowVirtualizer.getVirtualItems();
  const totalSize = rowVirtualizer.getTotalSize();

  if (results.length === 0) {
    return (
      <div className="empty-state">
        <div className="empty-icon">📊</div>
        <p>분석 결과가 없습니다.</p>
      </div>
    );
  }

  const colSizes = table.getHeaderGroups()[0]?.headers.map((h) => h.getSize()) ?? [];

  return (
    <div ref={scrollRef} className="analysis-table-wrap">
      <table style={{ tableLayout: 'fixed', width: '100%' }}>
        <colgroup>
          {colSizes.map((w, i) => (
            <col key={i} style={{ width: w }} />
          ))}
        </colgroup>
        <thead>
          {table.getHeaderGroups().map((hg) => (
            <tr key={hg.id}>
              {hg.headers.map((h) => (
                <th key={h.id} style={{ width: h.getSize() }}>
                  {flexRender(h.column.columnDef.header, h.getContext())}
                </th>
              ))}
            </tr>
          ))}
        </thead>
        <tbody
          style={{
            height: `${totalSize}px`,
            position: 'relative',
            display: 'block',
          }}
        >
          {virtualRows.map((virtualRow) => {
            const row = rows[virtualRow.index];
            const r = row.original;
            const isSelected = r.result_id === selectedId;
            return (
              <tr
                key={row.id}
                className={isSelected ? 'row-selected' : ''}
                onClick={() => onSelect(r)}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  transform: `translateY(${virtualRow.start}px)`,
                  display: 'table',
                  tableLayout: 'fixed',
                  boxSizing: 'border-box',
                }}
              >
                {row.getVisibleCells().map((cell) => (
                  <td key={cell.id} style={{ width: cell.column.getSize() }}>
                    {flexRender(cell.column.columnDef.cell, cell.getContext())}
                  </td>
                ))}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
