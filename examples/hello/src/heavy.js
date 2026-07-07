// Simulated heavy module (~large accumulated cost in real apps).
const data = [];
for (let i = 0; i < 5000; i++) {
  data.push(i * i);
}

export function heavy() {
  return data.reduce((a, b) => a + b, 0);
}
