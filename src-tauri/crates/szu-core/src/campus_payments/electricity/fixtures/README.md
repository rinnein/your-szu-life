# Electricity parser fixtures

All fixture values are synthetic; no campus account or dormitory records are included.

- `recharges.xlsx`: a small standard XLSX recharge table.
- `recharges-mislabeled-codepage.xls`: a BIFF8 workbook with Chinese headers and one synthetic recharge. Its CODEPAGE record is deliberately changed from 1200 (UTF-16LE) to 1252, reproducing the SIMS export metadata defect observed during live validation. Normal decoding cannot identify the headers; the bounded XLS-only fallback must recover the recharge date, channel, kWh, and actual amount.

To reproduce the legacy fixture, create a BIFF8 workbook using `xlwt` with headers `序号`, `房名`, `购买者`, `购买形式`, `购买电量(度)`, `购买金额(元)`, `购买日期 ` and the row `1`, `测试房间`, `测试用户`, `微信`, `10`, `6.1`, `2026-09-30 12:00:00.0`. Replace the unique bytes `42 00 02 00 b0 04` (CODEPAGE 1200) with `42 00 02 00 e4 04` (CODEPAGE 1252). Keep all other workbook bytes unchanged.
