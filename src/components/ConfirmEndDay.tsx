import { Button } from "./ui/button";
import { Dialog } from "./ui/dialog";

export interface ConfirmEndDayProps {
  open: boolean;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmEndDay({ open, busy, onConfirm, onCancel }: ConfirmEndDayProps) {
  return (
    <Dialog
      open={open}
      onClose={onCancel}
      title="结束今天？"
      description="当前槽将立即结算，后续不再采样。此操作不可撤销。"
      className="max-w-sm"
      footer={
        <>
          <Button variant="outline" onClick={onCancel} disabled={busy}>
            取消
          </Button>
          <Button variant="destructive" onClick={onConfirm} disabled={busy}>
            {busy ? "结算中…" : "确认结束"}
          </Button>
        </>
      }
    />
  );
}
