use super::ChatAssistant;

pub const DEFAULT_ID: &str = "asst_builtin_video_prompt";

const COMMON: &str = r#"你只负责把用户要求写成可直接交给视频模型的提示词，不生成视频、不调用工具、不解释写作过程。
用户的明确要求优先于你的风格偏好。时长、画幅、语言、声音模式与所选服务以本次输入为准；不要固定成15秒、四镜头、英语或某个平台格式。要求具体时直接写成品；要求简单时用一段，只有动作变化确实需要时才分镜。不要输出商品分析、痛点分析、策略、参数清单、重复的中英两版或大段禁止事项。
参考图片的用途由用户决定：商品外观、风格、场景或指定首尾帧。普通参考图不等于首帧；商品图旁的摆件和背景不自动成为拍摄要求。对复杂图案简短写“商品外观与参考图一致”，不要猜测或逐项复述。不得编造看不见的细节、功能、材质变化、认证、效果和操作声音。参考视频/音频只有路径时不可假装看过或听过；有拆解证据或模板时才依据其内容。
成品正文只写拍摄指令，不重述已经由工作台传递的分辨率、画幅等参数，不把用户的简单要求扩成多个栏目。商品外观统一引用参考图；用户没有用文字明确说明的材质、颜色、结构、发光部位等，不要通过看图推测再写进成品。镜头若要展示细节，写用户指定的细节目标即可，不补充细节属性。未要求改变布景与照明时保留现有环境与照明，只排除用户明确禁止的元素，不重新设计背景或改成夜景。
同一镜头的运动描述必须一致：固定机位不得同时推近、环绕或平移；用户要求推近就只写推近。输出前自行删掉无来源的外观断言、重复限制、通用负面词和额外剧情，不输出检查过程。
保留用户指定的动作、镜头、对白原文、音乐和禁忌。speechMode=dialogue 保留提供的台词；ambient 使用自然可听见的环境声，不写人声，不默认压成极轻微底噪；silent 不写任何声音；auto 按用户意图安排可听清的声音，不默认静音，也不强塞口播。language 只约束需要的口播语言，不意味着必须添加口播或字幕。
修改时只落实本次修改意见，保留未要求改变的内容，不恢复已删除的剧情或道具。输出中不承诺生成模型能绝对保证一致性。"#;

const UGC: &str = r#"你是电商 UGC 口播视频提示词助手。用户选中你，就已经选择了“真人出镜展示商品并说话”的创作方式，不需要用户在要求里再写一遍“真人、口播”。
当 speechMode=auto、用户没有明确要求无人或无口播时，直接输出一位成年人出镜展示商品、自然说出一句具体台词的成品提示词。短要求（例如“tk店铺展示视频”）也按此执行，不退回纯商品空镜，不返回拍法选项，不写“无人物、无手、无口播”。没有台词时你负责写一句符合语言设置的口语台词，不能只写“人物介绍商品”。
人物、简洁生活场景和拿起/展示商品的必要动作属于用户选择 UGC 的明确意图，不是擅自加戏。商品白底参考图只约束商品外观，不是必须复刻的拍摄场景；未指定场景时可选择一个与商品日常用途匹配的简单环境，不编产品历史或功效。不要照抄拍摄器材术语、商品规格清单或逐项外观描述。
时长短就减少动作与台词：5秒默认一个连贯镜头、一个易完成的展示动作、一句能自然说完的短句（中文通常约12至18字，英文约8至12词）。不安排开箱、试装、换机位等连续步骤，不靠加快语速塞长文案。时间更长时才按需要增加内容。说话的人要在画面中，脸与商品都清楚，口型同步，声音清晰，不用画外旁白替代真人口播。
用户给了台词就保留原文；未给台词时围绕已知商品种类和用户明确提供的卖点来写，不虚构购买经历、容量、舒适度、防水、耐用性、销量、库存或优惠。未指定口播语言时按 language；不固定英语或特定国家。
用户明确要求无口播，或 speechMode=ambient/silent 时，以该要求为准，不写人物说话；明确要求无人时不得安排出镜人物。修改时优先落实新的明确要求；旧稿与当前已选的UGC口播方式冲突时要纠正，不能机械保留旧稿的“无人、无口播”。
图片只用于识别商品类别和保持外观一致。成品写“商品外观与参考图一致”即可；不要从图推断材质，不逐项罗列颜色、徽章、口袋、拉链或其他外观。不能把可见口袋直接写成“好拿东西”“大容量”等未经用户提供的好处。没给卖点时用中性的款式介绍或邀请看看，不编功能利益。
5秒且未提供台词时，中文台词必须控制在18个汉字以内。例如背包可以说“看看这款背包，喜欢这个款式吗？”；例子只示范长度和不虚构卖点，不作为固定文案。正文直接写动作、台词和声音，不重复分辨率或画幅，不追加参数段。
最终只输出一段可直接生成的视频提示词，包含场景、人物与商品动作、具体台词及必要声音要求。动作写确定方案，不写“坐着或站着”“手拿或放桌上”等备选。只保留用户明确的限制，不用大段否定句占据正文。"#;

pub fn definitions(now: i64) -> Vec<ChatAssistant> {
    [
        (DEFAULT_ID, "通用视频", "按原意整理动作、镜头与声音，简单要求直接写成简洁提示词。", "你是通用视频提示词助手。忠实整理用户意图，只补足执行所需的衔接。不要为简单要求增加剧情、人物、道具、商品卖点或营销结尾。用户给了完整提示词时尽量保留原文。"),
        ("asst_builtin_video_product", "电商产品展示", "适合商品展示、细节特写和功能演示，以真实商品和明确卖点为准。", "你是电商产品展示视频提示词助手。围绕用户指定的商品与卖点，安排清楚的主体呈现、必要的细节特写和合理的镜头运动。仅根据已提供的事实演示功能。用户未指定时优先保持现有场景，用少量自然的镜头运动展示商品，不自行加入主持人、手部操作、拆装、配件或装饰道具。不要把图案与外观猜测写成长篇商品描述，不添加优惠、促销字幕、口号或购买引导。"),
        ("asst_builtin_video_ugc", "电商 UGC 口播", "默认真人展示商品并说出具体台词；自动声音下无需再写“要口播”。", UGC),
    ].into_iter().map(|(id, name, description, specialty)| ChatAssistant {
        id: id.into(), name: name.into(), description: description.into(),
        icon: "🎬".into(), color: "#5B4B8A".into(), source: "builtin".into(), category: "video".into(),
        system_prompt: if id == "asst_builtin_video_ugc" { specialty.into() } else { format!("{specialty}\n\n{COMMON}") },
        provider_id: String::new(), model: String::new(), mcp_server_ids: vec![], skill_ids: vec![],
        enabled: true, installed: false, archived: false, built_in: true, created_at: now, updated_at: now,
    }).collect()
}

/// This migration only replaces the old video persona and adds the two new ones.
/// Preserve all other assistants and the existing video assistant's model/state.
pub fn merge(mut existing: Vec<ChatAssistant>, now: i64) -> Vec<ChatAssistant> {
    for mut definition in definitions(now) {
        if let Some(slot) = existing.iter_mut().find(|a| a.id == definition.id) {
            definition.installed = slot.installed;
            definition.archived = slot.archived;
            definition.enabled = slot.enabled;
            definition.created_at = slot.created_at;
            definition.provider_id = slot.provider_id.clone();
            definition.model = slot.model.clone();
            *slot = definition;
        } else {
            existing.push(definition);
        }
    }
    existing
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ugc_upgrade_only_replaces_the_shipped_prompt_and_preserves_custom_edits() {
        let mut old = definitions(1).remove(2);
        old.system_prompt = include_str!("video-ugc-v10.txt").into();
        old.provider_id = "custom-provider".into();
        old.model = "custom-model".into();
        old.installed = true;
        let mut custom = old.clone();
        custom.system_prompt = "User edited prompt".into();
        let custom_before = serde_json::to_value(&custom).unwrap();
        let mut rows = vec![old];
        upgrade_ugc_v11(&mut rows, 2);
        assert!(rows[0].system_prompt.contains("用户选中你，就已经选择了"));
        assert!(!rows[0].system_prompt.contains("也不强塞口播"));
        assert_eq!(rows[0].provider_id, "custom-provider");
        assert_eq!(rows[0].model, "custom-model");
        assert!(rows[0].installed);
        let once = serde_json::to_value(&rows).unwrap();
        upgrade_ugc_v11(&mut rows, 3);
        assert_eq!(serde_json::to_value(&rows).unwrap(), once);
        upgrade_ugc_v11(std::slice::from_mut(&mut custom), 2);
        assert_eq!(serde_json::to_value(custom).unwrap(), custom_before);
    }

    #[test]
    fn migration_preserves_unrelated_assistants_and_video_model_state() {
        let mut old = definitions(1).remove(0);
        old.model = "my-vision-model".into();
        old.provider_id = "my-provider".into();
        old.installed = true;
        old.archived = true;
        let mut custom = old.clone();
        custom.id = "asst_custom".into();
        custom.system_prompt = "my prompt".into();
        let merged = merge(vec![old, custom.clone()], 2);
        assert_eq!(merged.len(), 4);
        assert_eq!(
            serde_json::to_value(&merged[1]).unwrap(),
            serde_json::to_value(custom).unwrap()
        );
        assert_eq!(merged[0].model, "my-vision-model");
        assert!(merged[0].archived && merged[0].installed);
        assert_eq!(merged[0].created_at, 1);
        assert_eq!(merge(merged, 2).len(), 4);
    }
}

/// Replace only the shipped v10 UGC prompt; preserve user edits and all settings.
pub fn upgrade_ugc_v11(assistants: &mut [ChatAssistant], now: i64) {
    for assistant in assistants {
        if assistant.id == "asst_builtin_video_ugc"
            && assistant.built_in
            && assistant.system_prompt == include_str!("video-ugc-v10.txt")
        {
            let updated = definitions(now).remove(2);
            assistant.system_prompt = updated.system_prompt;
            assistant.description = updated.description;
            assistant.updated_at = now;
        }
    }
}
