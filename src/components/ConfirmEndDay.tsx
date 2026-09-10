interface ConfirmEndDayProps {
  open: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmEndDay({ open, onConfirm, onCancel }: ConfirmEndDayProps) {
  if (!open) return null;
  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="modal">
        <h3>结束今天？</h3>
        <p>当前槽将立即结算，后续不再采样。此操作不可撤销。</p>
        <div className="modal-actions">
          <button type="button" onClick={onCancel}>取消</button>
          <button type="button" className="primary" onClick={onConfirm}>
            确认结束
          </button>
        </div>
      </div>
    </div>
  );
}
