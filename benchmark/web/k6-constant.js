// Constant-arrival-rate shape: 5,000 requests/s for the test duration.
// This substitutes for the article's `wrk2 -R 5000` measurement.
import http from "k6/http";

const target = __ENV.TARGET;
const duration = __ENV.DURATION || "10s";

export const options = {
  scenarios: {
    constant: {
      executor: "constant-arrival-rate",
      rate: 5000,
      timeUnit: "1s",
      duration,
      preAllocatedVUs: 100,
      maxVUs: 400,
    },
  },
};

export default function () {
  http.get(target);
}
