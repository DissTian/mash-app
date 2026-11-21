# 幕间物语自动化

实现位于 [runner/interlude_quest.rs](../../src-tauri/src/runner/interlude_quest.rs)，任务页为 [InterludePage.tsx](../../src/features/interlude/InterludePage.tsx)。本流程不读取项目/队伍，使用独立配置 `interlude_task.json`（`InterludeTaskConfig`）。

## 入口与配置

- 仅国服。主界面（01）→ 迦勒底之门 → 幕间物语 → 02 列表；也接受已停在 02/03-1。
- 独立配置：支援从者/礼装/等级/宝具/技能/星图、自动战斗方案、AP 恢复、自动编成、规定回合数任务、失败策略、连续失败次数、同任务最大重试、剧情宽限秒数。
- 设定支援从者时按该从者的 `className` 选择对应职阶页签；`className` 为空/未知时才回退到全职阶。未设定支援从者时不切页签。
- 启动时强制 `autoBattleTakeover`、跳过队伍校验、忽略五星礼装/羁绊结算停止项，并把 `supportClassFilterMode` 固定为全职阶。

## 画面与路由

| 画面 | 行为 |
| --- | --- |
| `Main` / `ChaldeaGate` | 滚动查找迦勒底之门/幕间物语入口；找不到则失败 |
| `InterludeQuest` / `InterludeStage` | 扫描当前可见任务行；有可点行就选最上面一个，否则向下滚动；到底后结束或返回 02 |
| `QuestConfirm` / `NoBattleQuestConfirm` | 点开始/任务开始 |
| `TurnLimitQuestConfirm` | 允许时点开始，否则取消并把标题加入已处理 |
| `FormationLimitDialog` | 关闭“编制限制”弹窗 |
| `AutoFormationDialog` | 按配置点自动编成/不进行自动编成 |
| `EmptySlotAutoFormationDialog` | 队伍有空位时点自动编成 |
| `FormationEditor` | 点决定 |
| `TeamConfirmInterlude` | 点战斗开始 |
| `TeamConfirm` | 队伍有空位（`选择`占位）时点底部自动编队，否则走普通开始任务 |
| `SupportSelect` | 有 NPC 角标优先点 NPC 行，否则走普通助战筛选 |
| `Dialog` / `DialogSkipConfirm` | 按选项框位置点最上面一项，否则尝试跳过；跳过确认点是 |
| `BattleContinue` | 令咒可用且配置令咒时使用令咒，否则撤退 |
| `CharmConfirm` | 点决定并重置战斗状态 |
| `RetreatNotice` | 点关闭 |
| `Battle` / `Attack` / 结算 | 复用普通战斗与结算流程；看到战斗或结算就清除进战斗等待标记，结算 Continue 关闭后等待返回列表 |
| `Unknown`（任务报酬） | 任务进行中或返回列表时，OCR 同时确认报酬标题和下方按钮后关闭；下一轮重新识别，允许连续弹出 |

## 任务列表

- `find_interlude_quest_rows` 以「消耗」锚点识别普通行、以「前往故事的舞台」锚点识别快捷行；「自由关卡」行不返回。
- 行标题区域生成 128-bit 图像指纹（`titleSignature`），用于单模式目标定位、已处理名单和重复保护；不依赖行号。
- 行可点击性由标题区域亮度判断；黑色/透明遮罩行不可点，跳过但不加入已处理。
- 全模式贪心扫描：当前可见行 → 选一个 → 回列表再扫；没有可点行就滚动。完成/撤退/取消的任务按标题跳过。
- 单模式只做截图选中的标题：在 02 或 03-1 按指纹匹配；目标消失视为完成，扫到列表底部仍未出现则失败。
- 多步任务（任务进行度）不判断进度、不解析箭头数：行还在且可点就会被再次选中，直到从列表消失。

## 对话与等待

- 主界面和迦勒底之门导航阶段不处理剧情；已识别的剧情页按未知画面等待。进入任务流程后，`engine.rs` 先探测剧情，再观察阶段和处理普通页面；不因画面被归为结算等其它页面就跳过探测。
- 发现剧情时重置未知画面和战斗加载等待计数，保留当前任务阶段。剧情及跳过确认均刷新宽限期（默认 15 秒）；宽限期内 `Unknown` 不进入战斗加载或普通画面超时逻辑。
- 选项按蓝色底边和深色框体定位，取最上方点击；角标模板作兜底，不依赖选项文字。没有选项才尝试模板或 OCR 找到的跳过按钮；按钮缺失或多次点击无效时点对话框推进。
- 无战斗任务点「任务开始」后直接进入返回等待。结算或尾声后若出现任务报酬弹窗，即使在剧情宽限期内也可以关闭；每次关闭后重新识别，不把一次关闭当作已回列表，持续未切换仍受超时限制。
- 跳过按钮/确认弹窗、失败页按钮、编队按钮均按模板/实测位置点击；令咒按钮先探测亮色模板，不可用强制撤退。

## 失败与结束

- 失败页撤退：当前标题加入已处理，继续下一个；令咒成功：重置 `BattleState` 继续战斗。
- 连续失败达到配置值停止；AP 恢复耗尽由现有 AP 流程结束。
- 全模式列表到底且没有可点行结束；结束时输出完成/跳过数量。
- 完成计数按“标题不再出现”近似统计；多步任务只计一次。

## 识别资源

- `cv.json` 新增 `Main`、`ChaldeaGate`、`InterludeQuest`、`InterludeStage`、三种确认弹窗、三种编队弹窗、`FormationEditor`、`TeamConfirmInterlude`、`Dialog`、`DialogSkipConfirm`、`BattleContinue`、`RetreatNotice`、`CharmConfirm`。
- 模板目录 `servers/cn/templates/interlude/`、`servers/cn/templates/main/`，均按 1920×1080 采集并设置 `templateReferenceWidth: 1920`（共享模板沿用 2560 参考）。
- 离线测试：`sidecar/cv-code/tests/test_cv.py` 使用 `tests/test_data/interlude/` 的 960×540 截图验证行识别、列表检测、剧情选项定位和跳过确认。
