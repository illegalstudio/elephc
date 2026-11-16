// 100 virtual users, each issuing one request every 100 ms.
// This is the k6 shape from the FrankenPHP-vs-PHP-FPM article.
import http from "k6/http";
import { sleep } from "k6";

const target = __ENV.TARGET;
const duration = __ENV.DURATION || "10s";

export const options = {
  vus: 100,
  duration,
};

export default function () {
  http.get(target);
  sleep(0.1);
}
