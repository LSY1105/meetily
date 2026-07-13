# Wave 11: Speaker Diarization (璇磋瘽浜哄垎绂? PR-41)

> **For agentic workers:** REQUIRED SUB-KILL: superpowers:subagent-driven-development (recommended) or superpowers:executing-plans.
> **Base branch:** feature/stability-wave8
> **Parent wave:** Wave 10 (recognition rate, 宸?ship)

## 鑳屾櫙

Wave 10 瑙ｅ喅浜嗗叧閿瘝璇嗗埆鐜囷紙hotwords + provider benchmark + perf 浼樺寲锛夈€備絾瀹屾暣鐨勪腑鏂囦細璁ā寮忎笅锛?
**鍗曡鐙珛杞啓闇€瑕佹樉绀鸿璇濅汉**锛?
- 2+ 浜轰細璁腑鏍囧紡璁板綍锛氥€滄湁浜鸿 XXX鈥濄€佲€滃彟涓€浣嶈 YYY鈥? 渚濇墭鏄剧ず璇磋瘽浜?
- 鐜板湪 UI 浠呮樉绀哄钩鍧囨枃鏈紝鏃犳硶鍒嗚鲸鐜?
- 鑱婂ぉ瀹℃牳 / 绾夸笂浼氳 / 璁板綍妫€鏌ユ牱鏅暣涓壊

## Goals

- 寤虹珛璇磋瘽浜哄垎绂荤殑 **UI 楠ㄦ灦**锛圥R-41a 璇ヤ换鍔★級
- 瀹屾暣鐨勫悗绔?diarization 鎺ㄧ悊鍚搁攣锛坧yannote / resemblyzer 浜屾墠閫夊嚭 PR-41b 鐪佺暀锛?
- 6 locale 鏍囧噯鏍囩О锛屽苟鏍″噯鏄剧ず鏍煎紡
- 涓嶇牬鍧?backward compatibility 鈥揥ithout speaker 鏁版嵁鏃剁伆鑹茶浆涓?unlabeled

## Non-Goals

- 涓嶅惎鐢ㄥ悗绔?ML 鎺ㄧ悊锛坧R-41b 鑼冨洿锛?
- 涓嶆浛鎹富 ASR 寮曟搸锛坉iarization 鏄悗澶勭悊绾э級
- 涓嶈瘑鍒瘽璇濅汉韬唤锛堜粎 "Speaker 1/2/3" 鏍囩ず锛?

## 閫夊瀷鍐崇瓟

| 閫夐」 | 浼樺姪 | 缂虹偣 | PR-41 鍐崇瓟 |
|---|---|---|---|
| pyannote.audio | SOTA 绮惧害 | 1.5GB+ torch 渚濊禆 | 缃?|
| **Resemblyzer + 骞茬郴** | ~30MB, 绾?numpy | DER ~25% | **鎺ㄨ崘 (PR-41b)** |
| 浜?API diarization | 绮惧害楂?| 涓嶅悎鏈湴浼樺厛鐞?| 缃?|
| silero-vad + 鑳介噺鍙樺寲 | 闆跺瓙 | 绮惧害浣?| 缃?|

**PR-41a 瀹炴柦鑼冨洿**锛堢函鍓嶇绔鑴氭牑锛?

1. 鏍囧瀷鎵╁睍锛歞ocs/superpowers/specs/2026-07-13-diarization-wave11.md 鍚庡彲杩藉姞
   - TranscriptSegmentData 娣诲姞 speaker?: string | null
   - 鍚戝悗鐩稿叧浜嬩欢 / 涓嬫父 / i18n 鎸佺画
2. UI 鏄剧ず璇磋瘽浜烘爣绛撅紙"Speaker 1/2/3" 鎴栬€呬綋瀹氬悕绉帮級
3. 6 locale i18n 鏂板?speaker_label / speaker_unknown / speaker_unknown_short
4. 涓嶇Щ鍔ㄥ悗绔?transcript_processor

**PR-41b 瀹炴柦鑼冨洿**锛堝悗缁?commits, 闇€瑕?pip install resemblyzer + torchaudio 鐜锛?

1. backend/app/diarization.py 鏂板缓
   - Resemblyzer VoiceEncoder 鎻愬彇 d-vector
   - sklearn SpectralClustering 鎸夎鑰匳AY 闆嗘帓
   - 杈撳嚭 [{start, end, speaker_id}, ...]
2. backend/app/transcript_processor.py 鎺ュ叆
   - 澧炲姞 enable_diarization 鍙傛暟
   - 杈撳嚭 segment 鏃舵寜鏃堕棿涓?speaker_id 鍐茬粰 transcript text
3. 閰嶇疆寮€鍏筹細UI 璁剧疆椤归槻鍛藉拰鐑瘝骞舵寕鏍?
4. 绔埌绔祴璇曪細5 鍒嗛挓娴嬭瘯闊宠皟鐢?WER + DER

## Architecture

### PR-41a 鎺ㄧ悊璺緞

```
[ASR provider] 鈫?[transcript text + segments (no speaker)]
                              鈫?[前端 TranscriptSegmentData + speaker?]
                                            鈫?[UI: speaker label or muted text]
```

**鏃犲悗绔?ML 鐨勬儏鍐典笅**, speaker 瀛楁涓衡€渘ull / undefined鈥?, UI 闅愯棌鏍囩О锛屼笉褰卞搷鐜扮姸銆?

### PR-41b 鎺ㄧ悊璺緞 (鐪熸ā寮?

```
[ASR provider] 鈫?[transcript text + segments]
                              鈫?[backend/app/diarization.py 璋冪敤]
                                            鈫?[enrichment: 娣诲姞 speaker_id]
                                                      鈫?[SQLite store]
                                                                  鈫?[API response]
                                                                              鈫?[UI 鏍囩ず]
```

### PR-41a 鏂囦欢鍙樻洿

- `frontend/src/types/index.ts` 鈥?TranscriptSegmentData 娣诲姞 speaker?
- `frontend/src/components/VirtualizedTranscriptView.tsx` 鈥?TranscriptSegment 鎺ュ彈 speaker
- `frontend/locales/{6 locale}/settings.json + common.json` 鈥?speaker_label / speaker_unknown

### PR-41b 鏂囦欢鍙樻洿

- `backend/app/diarization.py` 鈥?鏂板缓 (Resemblyzer + sklearn)
- `backend/app/transcript_processor.py` 鈥?鎺ュ叆 diarization
- `backend/app/main.py` 鈥?API 鎺ュ叆
- `backend/requirements.txt` 鈥?resemblyzer + torchaudio

## Risks

| 椋庨櫓 | 绛规爣 | 绛戦噴 |
|---|---|---|
| Resemblyzer 瀵逛腑鏂囦笉澶ソ | 绮惧害 DER > 30% | 鍑?PR-41c 鎹?pyannote ONNX |
| Windows + torchaudio 渚濊禆 | torchaudio wheel 澶辫触 | 鐣欑粰鐢ㄦ埛閫夋嫨 local installer 鎸囧崡 |
| 姝绘尝璇嗗埆 | 鐭殏鍋滄粸/鐢蜂簤 | 浣庢晱鎰熷甫 i18n 鏍囩О "..." |

## Compatibility

- 鏃?speaker 瀛楁鏃讹紝UI 鏄剧ず涓?unlabeled (杈冨皝闂?UI 缁曡繖澧炵矖鐨勮繃鐢?
- 鏃?speaker 瀛楁鏃讹紝UI 鏍囩ず璇磋瘽浜?
- 鏍″噯浜?API 濡傛灉娌℃湁 speaker 瀛楁锛屼笉褰卞搷涓氬姟娴侀?

## Acceptance (PR-41a)

- [ ] TranscriptSegmentData 绫诲瀷瀹氫箟鏂?speaker?
- [ ] TranscriptSegment 缁勪欢鏄剧ず speaker 鏍囩О
- [ ] VirtualizedTranscriptView 浼犻€?speaker 缁?TranscriptSegment
- [ ] 6 locale 鏂板?speaker_label / speaker_unknown 閿?
- [ ] `pnpm check:i18n` 閫?pass
- [ ] `pnpm test:i18n` 閫?pass (19+ tests)
- [ ] `pnpm build` 閫?pass (11+ static pages)

## References

- Resemblyzer: https://github.com/resemble-ai/Resemblyzer
- pyannote.audio: https://github.com/pyannote/pyannote-audio
- sklearn SpectralClustering: https://scikit-learn.org/stable/modules/generated/sklearn.cluster.SpectralClustering.html
- Wave 10 spec: docs/superpowers/specs/2026-07-13-recognition-wave10.md
