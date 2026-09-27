// 第四版时间轴：画面（jpp-intro.html）与配乐（music.py）共用。每场按整小节计，切换点落在重拍上。
window.TIMELINE = {
  "bpm": 96,
  "beatsPerBar": 4,
  "scenes": [
    { "id": "jev", "bars": 3, "music": "intro" },
    { "id": "jpp", "bars": 4, "music": "intro" },
    { "id": "eight", "bars": 6, "music": "tension" },
    { "id": "takeover", "bars": 2, "music": "reveal" },
    { "id": "cap-route", "bars": 3, "music": "groove" },
    { "id": "cap-sieve", "bars": 3, "music": "groove" },
    { "id": "cap-pair", "bars": 3, "music": "groove" },
    { "id": "cap-search", "bars": 3, "music": "groove" },
    { "id": "cap-tree", "bars": 3, "music": "groove" },
    { "id": "effects", "bars": 5, "music": "lift" },
    { "id": "scale", "bars": 4, "music": "lift" },
    { "id": "cta", "bars": 5, "music": "outro" }
  ]
};
if (typeof module !== "undefined") module.exports = window.TIMELINE;
