import { ContextMenu, ContextMenuItem, ContextMenuSub } from "./ui/context-menu";
import type { TaskListView, TaskView } from "../lib/api";

export function TaskActionMenu({
  open,
  x,
  y,
  task,
  lists,
  onClose,
  onDate,
  onMove,
  onDuplicate,
  onAbandon,
}: {
  open: boolean;
  x: number;
  y: number;
  task: TaskView | null;
  lists: TaskListView[];
  onClose: () => void;
  onDate: (task: TaskView) => void;
  onMove: (task: TaskView, listId: string) => void;
  onDuplicate: (task: TaskView) => void;
  onAbandon: (task: TaskView) => void;
}) {
  const others = task ? lists.filter((list) => list.id !== task.listId) : [];
  return (
    <ContextMenu open={open && task != null} x={x} y={y} onClose={onClose}>
      {task && (
        <>
          <ContextMenuItem onSelect={() => onDate(task)}>更改日期…</ContextMenuItem>
          {others.length > 0 && (
            <ContextMenuSub label="移动到">
              {others.map((list) => (
                <ContextMenuItem
                  key={list.id}
                  onSelect={() => onMove(task, list.id)}
                >
                  {list.name}
                </ContextMenuItem>
              ))}
            </ContextMenuSub>
          )}
          <ContextMenuItem onSelect={() => onDuplicate(task)}>创建副本</ContextMenuItem>
          <ContextMenuItem destructive onSelect={() => onAbandon(task)}>
            放弃任务
          </ContextMenuItem>
        </>
      )}
    </ContextMenu>
  );
}

export function TaskBulkMenu({
  open,
  x,
  y,
  lists,
  onClose,
  onMove,
  onAbandon,
}: {
  open: boolean;
  x: number;
  y: number;
  lists: TaskListView[];
  onClose: () => void;
  onMove: (listId: string) => void;
  onAbandon: () => void;
}) {
  return (
    <ContextMenu open={open} x={x} y={y} onClose={onClose}>
      {lists.length > 0 && (
        <ContextMenuSub label="移动到">
          {lists.map((list) => (
            <ContextMenuItem key={list.id} onSelect={() => onMove(list.id)}>
              {list.name}
            </ContextMenuItem>
          ))}
        </ContextMenuSub>
      )}
      <ContextMenuItem destructive onSelect={onAbandon}>
        放弃任务
      </ContextMenuItem>
    </ContextMenu>
  );
}
