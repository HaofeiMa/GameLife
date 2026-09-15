import type { CategoryGuides } from "./api";

export const GUIDE_PLACEHOLDERS = {
  mainline: "当天任务页主线分组里的科研任务，以及在 Cursor、论文 PDF、Overleaf 上写代码、改稿、看文献。",
  side: "与当天主线课题无直接关系的工具、个人项目、整理仓库、打磨 GameLife。",
  admin: "邮件、报销、填表、组会行政、改个人主页。",
  entertainment: "视频、社交媒体、购物、无目的刷网。B 站和 YouTube 默认算娱乐。",
} as const;

export function savedCategoryGuides(guides: CategoryGuides): CategoryGuides {
  return {
    mainline: guides.mainline.trim(),
    side: guides.side.trim(),
    admin: guides.admin.trim(),
    entertainment: guides.entertainment.trim(),
  };
}
