import { TaskDateCard } from "./TaskDateCard";
import { Dialog } from "./ui/dialog";
import type { TaskView } from "../lib/api";

export function TaskDateDialog({
  task,
  onClose,
  onSaved,
  onError,
  onWarning,
}: {
  task: TaskView | null;
  onClose: () => void;
  onSaved: () => Promise<void> | void;
  onError: (message: string) => void;
  onWarning?: (text: string) => void;
}) {
  if (!task) {
    return <Dialog open={false} onClose={onClose} title="更改日期" chrome="float" />;
  }
  return (
    <Dialog open onClose={onClose} title="更改日期" chrome="float">
      <TaskDateCard
        key={task.id}
        task={task}
        onClose={onClose}
        onSaved={onSaved}
        onError={onError}
        onWarning={onWarning}
      />
    </Dialog>
  );
}
