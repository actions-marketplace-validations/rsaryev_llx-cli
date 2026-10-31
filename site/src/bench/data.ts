// The landing's benchmark boards, as they were last measured.
import type { Board } from './types';

export const TASK = "task-tracker";
export const BOARDS: Board[] = [
  {
    "id": "claude-haiku-5.5",
    "model": "Claude Haiku 5.5",
    "stamp": "2026-10-30T15-54-32",
    "effort": "medium",
    "rows": [
      {
        "agent": "llx",
        "ours": true,
        "runs": 1,
        "score": 199,
        "low": 199,
        "high": 199,
        "total": 199,
        "seconds": 329,
        "cost": 0.0595,
        "tokens": 1448498,
        "version": "0.2.0",
        "stamp": "2026-10-30T15-54-32",
        "effort": "medium",
        "trials": [
          {
            "seconds": 328.7,
            "score": 199,
            "cost": 0.0595,
            "tokens": 1448498
          }
        ]
      },
      {
        "agent": "Claude Code",
        "ours": false,
        "runs": 3,
        "score": 199,
        "low": 198,
        "high": 199,
        "total": 199,
        "seconds": 457,
        "cost": 0.0816,
        "tokens": 1323794,
        "version": "2.1.314",
        "stamp": "2026-10-30T15-54-32",
        "effort": "medium",
        "trials": [
          {
            "seconds": 454.4,
            "score": 199,
            "cost": 0.0817,
            "tokens": 1323794
          },
          {
            "seconds": 457.4,
            "score": 199,
            "cost": 0.0785,
            "tokens": 863458
          },
          {
            "seconds": 537.4,
            "score": 198,
            "cost": 0.0846,
            "tokens": 1378934
          }
        ]
      },
      {
        "agent": "opencode",
        "ours": false,
        "runs": 3,
        "score": 5,
        "low": 5,
        "high": 5,
        "total": 199,
        "seconds": 174,
        "cost": 0.0224,
        "tokens": 120195,
        "version": "1.19.5",
        "stamp": "2026-10-30T15-54-32",
        "effort": "medium",
        "trials": [
          {
            "seconds": 171.8,
            "score": 5,
            "cost": 0.0212,
            "tokens": 120195
          },
          {
            "seconds": 173.5,
            "score": 5,
            "cost": 0.0228,
            "tokens": 115333
          },
          {
            "seconds": 181.0,
            "score": 5,
            "cost": 0.0233,
            "tokens": 150495
          }
        ]
      },
      {
        "agent": "Hermes Agent",
        "ours": false,
        "runs": 3,
        "score": 5,
        "low": 5,
        "high": 198,
        "total": 199,
        "seconds": 1106,
        "cost": 0.4729,
        "tokens": 4388284,
        "version": "0.22.5",
        "stamp": "2026-10-30T15-54-32",
        "effort": "medium",
        "trials": [
          {
            "seconds": 394.0,
            "score": 198,
            "cost": 0.4729,
            "tokens": 4388284
          },
          {
            "seconds": 1105.9,
            "score": 5,
            "cost": null,
            "tokens": null
          },
          {
            "seconds": 1110.6,
            "score": 5,
            "cost": null,
            "tokens": null
          }
        ]
      }
    ]
  },
  {
    "id": "gpt-6-luna",
    "model": "GPT-6 Luna",
    "stamp": "2026-10-31T14-29-22",
    "effort": "default",
    "rows": [
      {
        "agent": "llx",
        "ours": true,
        "runs": 1,
        "score": 197,
        "low": 197,
        "high": 197,
        "total": 199,
        "seconds": 96,
        "cost": 0.0124,
        "tokens": 416459,
        "version": "0.2.0",
        "stamp": "2026-10-31T14-29-22",
        "effort": "default",
        "trials": [
          {
            "seconds": 96.1,
            "score": 197,
            "cost": 0.0124,
            "tokens": 416459
          }
        ]
      },
      {
        "agent": "Codex CLI",
        "ours": false,
        "runs": 2,
        "score": 197.5,
        "low": 196,
        "high": 199,
        "total": 199,
        "seconds": 111,
        "cost": 0.0264,
        "tokens": 1519777.0,
        "version": "0.163.0",
        "stamp": "2026-10-31T14-29-22",
        "effort": "default",
        "trials": [
          {
            "seconds": 96.2,
            "score": 199,
            "cost": 0.02,
            "tokens": 916685
          },
          {
            "seconds": 125.5,
            "score": 196,
            "cost": 0.0328,
            "tokens": 2122869
          }
        ]
      },
      {
        "agent": "opencode",
        "ours": false,
        "runs": 2,
        "score": 198.5,
        "low": 198,
        "high": 199,
        "total": 199,
        "seconds": 206,
        "cost": 0.0273,
        "tokens": 1117352.0,
        "version": "1.19.5",
        "stamp": "2026-10-31T14-29-22",
        "effort": "default",
        "trials": [
          {
            "seconds": 194.2,
            "score": 198,
            "cost": 0.0261,
            "tokens": 1024434
          },
          {
            "seconds": 216.9,
            "score": 199,
            "cost": 0.0285,
            "tokens": 1210270
          }
        ]
      },
      {
        "agent": "Hermes Agent",
        "ours": false,
        "runs": 3,
        "score": 198,
        "low": 198,
        "high": 199,
        "total": 199,
        "seconds": 213,
        "cost": 0.0396,
        "tokens": 1776726,
        "version": "0.22.5",
        "stamp": "2026-10-30T15-54-59",
        "effort": "medium",
        "trials": [
          {
            "seconds": 176.5,
            "score": 198,
            "cost": 0.034,
            "tokens": 1776726
          },
          {
            "seconds": 212.9,
            "score": 198,
            "cost": 0.0369,
            "tokens": 1530775
          },
          {
            "seconds": 331.9,
            "score": 199,
            "cost": 0.0479,
            "tokens": 2351587
          }
        ]
      }
    ]
  },
  {
    "id": "deepseek-v4.1-flash-1017",
    "model": "DeepSeek V4.1 Flash 1017",
    "stamp": "2026-10-31T02-14-24",
    "effort": "high",
    "rows": [
      {
        "agent": "llx",
        "ours": true,
        "runs": 1,
        "score": 199,
        "low": 199,
        "high": 199,
        "total": 199,
        "seconds": 230,
        "cost": 0.0406,
        "tokens": 1112418,
        "version": "0.2.0",
        "stamp": "2026-10-31T02-14-24",
        "effort": "high",
        "trials": [
          {
            "seconds": 229.7,
            "score": 199,
            "cost": 0.0406,
            "tokens": 1112418
          }
        ]
      },
      {
        "agent": "opencode",
        "ours": false,
        "runs": 3,
        "score": 199,
        "low": 199,
        "high": 199,
        "total": 199,
        "seconds": 244,
        "cost": 0.0361,
        "tokens": 1516396,
        "version": "1.19.5",
        "stamp": "2026-10-30T15-55-00",
        "effort": "high",
        "trials": [
          {
            "seconds": 234.4,
            "score": 199,
            "cost": 0.0352,
            "tokens": 1516396
          },
          {
            "seconds": 244.3,
            "score": 199,
            "cost": 0.0334,
            "tokens": 1864912
          },
          {
            "seconds": 276.2,
            "score": 199,
            "cost": 0.0397,
            "tokens": 700162
          }
        ]
      },
      {
        "agent": "Hermes Agent",
        "ours": false,
        "runs": 3,
        "score": 198,
        "low": 198,
        "high": 198,
        "total": 199,
        "seconds": 266,
        "cost": 0.0458,
        "tokens": 3408200,
        "version": "0.22.5",
        "stamp": "2026-10-30T15-55-00",
        "effort": "high",
        "trials": [
          {
            "seconds": 250.7,
            "score": 198,
            "cost": 0.0417,
            "tokens": 1832201
          },
          {
            "seconds": 266.0,
            "score": 198,
            "cost": 0.045,
            "tokens": 3408200
          },
          {
            "seconds": 312.4,
            "score": 198,
            "cost": 0.0508,
            "tokens": 3538319
          }
        ]
      }
    ]
  }
];
