import { open } from '@tauri-apps/plugin-dialog';

interface Props {
  rootPath: string;
  onChange: (path: string) => void;
  disabled?: boolean;
}

export default function FolderSelector({ rootPath, onChange, disabled }: Props) {
  const handleSelect = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: '프로젝트 루트 폴더 선택',
    });
    if (typeof selected === 'string' && selected) {
      onChange(selected);
    }
  };

  return (
    <div className="folder-selector">
      <div className={`folder-path ${rootPath ? 'has-path' : ''}`}>
        {rootPath || '프로젝트 루트 폴더를 선택하세요'}
      </div>
      <button className="btn btn-secondary" onClick={handleSelect} disabled={disabled}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
        </svg>
        폴더 선택
      </button>
    </div>
  );
}
