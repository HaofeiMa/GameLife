import { describe, expect, it } from "vitest";
import { liveMatchLabel, type LiveWindow, type QuestView } from "./questLive";

const quests: QuestView[] = [
  { text: "HDP", evidence: ["HDP"], hero: true },
];

const live = (over: Partial<LiveWindow>): LiveWindow => ({
  app: "Cursor",
  title: "train.py — HDP",
  documentPath: null,
  url: null,
  matchedQuestIndex: 0,
  trusted: true,
  ...over,
});

describe("liveMatchLabel", () => {
  it("no sample", () => {
    expect(liveMatchLabel(null, quests)).toBe("尚无观测");
  });
  it("miss", () => {
    expect(liveMatchLabel(live({ matchedQuestIndex: null }), quests)).toBe(
      "当前窗口未命中 Quest",
    );
  });
  it("trusted hit", () => {
    expect(liveMatchLabel(live({}), quests)).toBe("命中「HDP」");
  });
  it("untrusted hit", () => {
    expect(liveMatchLabel(live({ trusted: false }), quests)).toBe(
      "命中「HDP」，当前应用不在 Trusted，不会记入主线",
    );
  });
});
