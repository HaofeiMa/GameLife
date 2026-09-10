interface Progress32Props {
  haveSecs: number;
}

const CELLS = 32;
const CELL_SECS = 900;

export function Progress32({ haveSecs }: Progress32Props) {
  const fullCells = Math.min(CELLS, Math.floor(haveSecs / CELL_SECS));
  const partial = (haveSecs % CELL_SECS) / CELL_SECS;

  return (
    <div className="progress32" role="progressbar" aria-valuenow={haveSecs}>
      {Array.from({ length: CELLS }, (_, i) => {
        let fill = 0;
        if (i < fullCells) fill = 1;
        else if (i === fullCells && fullCells < CELLS) fill = partial;
        return (
          <div key={i} className="progress32-cell" title={`格 ${i + 1}`}>
            <div className="progress32-fill" style={{ width: `${fill * 100}%` }} />
          </div>
        );
      })}
    </div>
  );
}
