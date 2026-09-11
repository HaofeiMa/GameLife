export interface QuestView {
  text: string;
  evidence: string[];
  hero: boolean;
}

export interface LiveWindow {
  app: string;
  title: string;
  documentPath: string | null;
  url: string | null;
  matchedQuestIndex: number | null;
  trusted: boolean;
}

export function liveMatchLabel(
  live: LiveWindow | null,
  quests: QuestView[],
): string {
  if (live === null) {
    return "尚无观测";
  }
  if (live.matchedQuestIndex === null) {
    return "当前窗口未命中 Quest";
  }
  const quest = quests[live.matchedQuestIndex];
  if (!quest) {
    return "当前窗口未命中 Quest";
  }
  if (live.trusted) {
    return `命中「${quest.text}」`;
  }
  return `命中「${quest.text}」，当前应用不在 Trusted，不会记入主线`;
}
