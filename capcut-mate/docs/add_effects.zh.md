# ADD_EFFECTS API 接口文档

## 🌐 语言切换
[中文版](./add_audios.zh.md) | [English](./add_audios.md)

## 接口信息

```bash
POST /openapi/capcut-mate/v1/add_effects
```

## 功能描述

向现有草稿中添加视频特效。该接口用于在指定的时间段内添加特效素材到剪映草稿中，支持多种特效类型如边框特效、滤镜特效、动态特效等。特效可以用于增强视频的视觉效果。

## 更多文档

📖 更多详细文档和教程请访问：[https://docs.jcaigc.cn](https://docs.jcaigc.cn)

## 请求参数

```json
{
  "draft_url": "https://capcut-mate.jcaigc.cn/openapi/capcut-mate/v1/get_draft?draft_id=2025092811473036584258",
  "effect_infos": "[{\"effect_title\": \"录制边框 III\", \"start\": 0, \"end\": 5000000}, {\"effect_title\": \"复古滤镜\", \"start\": 2000000, \"end\": 7000000}]"
}
```

### 参数说明

| 参数名 | 类型 | 必填 | 默认值 | 说明 |
|--------|------|------|--------|------|
| draft_url | string | ✅ | - | 目标草稿的完整URL |
| effect_infos | string | ✅ | - | 特效信息列表的JSON字符串 |

### 参数详解

#### effect_infos 字段格式

`effect_infos` 是一个JSON字符串，包含特效信息数组，每个特效对象包含以下字段：

```json
[
    {
        "effect_title": "录制边框 III",  // 特效名称/标题，必选参数
        "start": 0,                     // 特效开始时间（微秒），必选参数  
        "end": 5000000                  // 特效结束时间（微秒），必选参数
    }
]
```

**字段说明**:
- `effect_title`: 特效名称，必须是系统中已存在的特效名称
- `start`: 特效开始时间，单位为微秒，必须大于等于0
- `end`: 特效结束时间，单位为微秒，必须大于start

#### 时间参数

- **start**: 特效在时间轴上的开始时间，单位为微秒（1秒 = 1,000,000微秒）
- **end**: 特效在时间轴上的结束时间，单位为微秒
- **duration**: 特效显示时长 = end - start

#### 特效名称说明

- **effect_title**: 特效的名称
  - 格式：字符串
  - 示例：`"录制边框 III"`
  - 获取方式：通过剪映特效库或相关API获取
  - 常见特效名称：
    - 边框特效："录制边框 III", "简约边框", "霓虹边框"
    - 滤镜特效："复古滤镜", "黑白滤镜", "暖色调"
    - 动态特效："粒子效果", "光晕效果", "闪烁特效"
    - 转场特效："淡入淡出", "推拉门", "马赛克转场"

## 响应格式

### 成功响应 (200)

```json
{
  "draft_url": "https://capcut-mate.jcaigc.cn/openapi/capcut-mate/v1/get_draft?draft_id=2025092811473036584258",
  "track_id": "effect_track_123",
  "effect_ids": ["effect_001", "effect_002"],
  "segment_ids": ["seg_001", "seg_002"]
}
```

### 响应字段说明

| 字段名 | 类型 | 说明 |
|--------|------|------|
| draft_url | string | 更新后的草稿URL |
| track_id | string | 特效轨道ID |
| effect_ids | array | 添加的特效ID列表 |
| segment_ids | array | 创建的特效片段ID列表 |

### 错误响应 (4xx/5xx)

```json
{
  "detail": "错误信息描述"
}
```

## 使用示例

### cURL 示例

#### 1. 基本特效添加

```bash
curl -X POST https://capcut-mate.jcaigc.cn/openapi/capcut-mate/v1/add_effects \
  -H "Content-Type: application/json" \
  -d '{
    "draft_url": "YOUR_DRAFT_URL",
    "effect_infos": "[{\"effect_title\": \"录制边框 III\", \"start\": 0, \"end\": 5000000}]"
  }'
```

#### 2. 批量特效添加

```bash
curl -X POST https://capcut-mate.jcaigc.cn/openapi/capcut-mate/v1/add_effects \
  -H "Content-Type: application/json" \
  -d '{
    "draft_url": "YOUR_DRAFT_URL",
    "effect_infos": "[{\"effect_title\": \"录制边框 III\", \"start\": 0, \"end\": 5000000}, {\"effect_title\": \"复古滤镜\", \"start\": 2000000, \"end\": 7000000}]"
  }'
```

## 错误码说明

| 错误码 | 错误信息 | 说明 | 解决方案 |
|--------|----------|------|----------|
| 400 | draft_url是必填项 | 缺少草稿URL参数 | 提供有效的draft_url |
| 400 | effect_infos是必填项 | 缺少特效信息参数 | 提供有效的effect_infos |
| 400 | 时间范围无效 | end必须大于start | 确保结束时间大于开始时间 |
| 400 | 无效的特效信息，请检查effect_infos字段值是否正确 | 特效参数校验失败 | 检查特效参数是否符合要求 |
| 404 | 草稿不存在 | 指定的草稿URL无效 | 检查草稿URL是否正确 |
| 404 | 特效不存在 | 指定的特效名称无效 | 确认特效名称是否正确 |
| 500 | 特效添加失败 | 内部处理错误 | 联系技术支持 |

## 注意事项

1. **时间单位**: 所有时间参数使用微秒（1秒 = 1,000,000微秒）
2. **特效名称**: 确保使用有效的特效名称
3. **时间范围**: end必须大于start
4. **轨道管理**: 系统自动创建特效轨道
5. **性能考虑**: 避免同时添加大量特效

## 工作流程

1. 验证必填参数（draft_url, effect_infos）
2. 检查时间范围的有效性
3. 从缓存中获取草稿
4. 创建特效轨道（如果不存在）
5. 解析特效信息并创建特效片段
6. 添加片段到轨道
7. 保存草稿
8. 返回特效信息

## 相关接口

- [创建草稿](./create_draft.md)
- [添加视频](./add_videos.md)
- [添加音频](./add_audios.md)
- [添加图片](./add_images.md)
- [保存草稿](./save_draft.md)
- [生成视频](./gen_video.md)

---

<div align="right">

📚 **项目资源**  
**GitHub**: [https://github.com/Hommy-master/capcut-mate](https://github.com/Hommy-master/capcut-mate)  
**Gitee**: [https://gitee.com/taohongmin-gitee/capcut-mate](https://gitee.com/taohongmin-gitee/capcut-mate)

</div>