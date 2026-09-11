import { useEffect, useState } from "react";
import { getWeek, type WeekView } from "../lib/api";

const ROWS: { key: keyof WeekView; label: string }[] = [
  { key: "core", label: "Core" },
  { key: "support", label: "Support" },
  { key: "admin", label: "Admin" },
  { key: "side", label: "Side Projects" },
  { key: "distraction", label: "Distraction" },
  { key: "away", label: "Away" },
  { key: "unobserved", label: "Unobserved" },
  { key: "pendingReview", label: "未复核" },
];

export function Week() {
  const [data, setData] = useState<WeekView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getWeek()
      .then((w) => {
        setData(w);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, []);

  if (error) {
    return (
      <div className="page">
        <p className="error">{error}</p>
        <button
          type="button"
          onClick={() =>
            getWeek()
              .then((w) => {
                setData(w);
                setError(null);
              })
              .catch((e) => setError(String(e)))
          }
        >
          重试
        </button>
      </div>
    );
  }
  if (!data) return <p className="muted">加载中…</p>;

  return (
    <div className="page">
      <h2>本周</h2>
      <p className="muted">按 activity 秒数求和，显示整数分钟</p>
      <table className="week-table">
        <thead>
          <tr>
            <th>类别</th>
            <th>分钟</th>
          </tr>
        </thead>
        <tbody>
          {ROWS.map(({ key, label }) => (
            <tr key={key}>
              <td>{label}</td>
              <td>{data[key] as number}m</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
