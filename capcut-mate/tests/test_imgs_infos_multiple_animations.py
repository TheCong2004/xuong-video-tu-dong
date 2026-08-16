import json
import sys
import os

# 添加项目根目录到Python路径
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))

from src.service.imgs_infos import imgs_infos


def test_multiple_animations_basic():
    """测试基本的多动画功能"""
    print("测试基本的多动画功能:")
    
    imgs = [
        "https://example.com/image1.jpg",
        "https://example.com/image2.jpg", 
        "https://example.com/image3.jpg"
    ]
    
    timelines = [
        {"start": 0, "end": 1000000},
        {"start": 1000000, "end": 2000000},
        {"start": 2000000, "end": 3000000}
    ]
    
    # 测试多个入场动画
    in_animation = "淡入|展开|缩放"
    in_animation_duration = 500000
    
    infos_json = imgs_infos(
        imgs=imgs,
        timelines=timelines,
        in_animation=in_animation,
        in_animation_duration=in_animation_duration
    )
    
    infos = json.loads(infos_json)
    print(f"生成的图片信息: {infos}")
    
    # 验证每个图片都获得了对应的动画
    assert len(infos) == 3
    assert infos[0]["in_animation"] == "淡入"
    assert infos[1]["in_animation"] == "展开" 
    assert infos[2]["in_animation"] == "缩放"
    assert infos[0]["in_animation_duration"] == 500000
    assert infos[1]["in_animation_duration"] == 500000
    assert infos[2]["in_animation_duration"] == 500000
    
    print("✓ 基本多动画测试通过")


def test_animation_extension_logic():
    """测试动画扩展逻辑：动画不足时使用最后一个"""
    print("\n测试动画扩展逻辑:")
    
    imgs = [
        "https://example.com/image1.jpg",
        "https://example.com/image2.jpg",
        "https://example.com/image3.jpg",
        "https://example.com/image4.jpg"
    ]
    
    timelines = [
        {"start": 0, "end": 1000000},
        {"start": 1000000, "end": 2000000},
        {"start": 2000000, "end": 3000000},
        {"start": 3000000, "end": 4000000}
    ]
    
    # 只提供2个动画，但有4张图片
    in_animation = "淡入|展开"
    in_animation_duration = 500000
    
    infos_json = imgs_infos(
        imgs=imgs,
        timelines=timelines,
        in_animation=in_animation,
        in_animation_duration=in_animation_duration
    )
    
    infos = json.loads(infos_json)
    print(f"生成的图片信息: {infos}")
    
    # 验证扩展逻辑：前两个使用指定动画，后两个使用最后一个动画
    assert len(infos) == 4
    assert infos[0]["in_animation"] == "淡入"
    assert infos[1]["in_animation"] == "展开"
    assert infos[2]["in_animation"] == "展开"  # 使用最后一个动画
    assert infos[3]["in_animation"] == "展开"  # 使用最后一个动画
    
    print("✓ 动画扩展逻辑测试通过")


def test_excess_animations():
    """测试动画过多时的处理：忽略多余的动画"""
    print("\n测试动画过多时的处理:")
    
    imgs = [
        "https://example.com/image1.jpg",
        "https://example.com/image2.jpg"
    ]
    
    timelines = [
        {"start": 0, "end": 1000000},
        {"start": 1000000, "end": 2000000}
    ]
    
    # 提供3个动画，但只有2张图片
    in_animation = "淡入|展开|缩放|旋转"
    in_animation_duration = 500000
    
    infos_json = imgs_infos(
        imgs=imgs,
        timelines=timelines,
        in_animation=in_animation,
        in_animation_duration=in_animation_duration
    )
    
    infos = json.loads(infos_json)
    print(f"生成的图片信息: {infos}")
    
    # 验证只使用前两个动画
    assert len(infos) == 2
    assert infos[0]["in_animation"] == "淡入"
    assert infos[1]["in_animation"] == "展开"
    # 多余的动画(缩放、旋转)应该被忽略
    
    print("✓ 动画过多处理测试通过")


def test_multiple_animation_types():
    """测试多种动画类型同时使用"""
    print("\n测试多种动画类型同时使用:")
    
    imgs = [
        "https://example.com/image1.jpg",
        "https://example.com/image2.jpg",
        "https://example.com/image3.jpg"
    ]
    
    timelines = [
        {"start": 0, "end": 1000000},
        {"start": 1000000, "end": 2000000},
        {"start": 2000000, "end": 3000000}
    ]
    
    # 同时测试三种动画类型
    in_animation = "淡入|展开|缩放"
    in_animation_duration = 500000
    loop_animation = "呼吸|旋转|闪烁"
    loop_animation_duration = 1000000
    out_animation = "淡出|收缩|翻转"
    out_animation_duration = 300000
    
    infos_json = imgs_infos(
        imgs=imgs,
        timelines=timelines,
        in_animation=in_animation,
        in_animation_duration=in_animation_duration,
        loop_animation=loop_animation,
        loop_animation_duration=loop_animation_duration,
        out_animation=out_animation,
        out_animation_duration=out_animation_duration
    )
    
    infos = json.loads(infos_json)
    print(f"生成的图片信息: {infos}")
    
    # 验证所有动画类型都正确分配
    assert len(infos) == 3
    assert infos[0]["in_animation"] == "淡入"
    assert infos[0]["loop_animation"] == "呼吸"
    assert infos[0]["out_animation"] == "淡出"
    
    assert infos[1]["in_animation"] == "展开"
    assert infos[1]["loop_animation"] == "旋转"
    assert infos[1]["out_animation"] == "收缩"
    
    assert infos[2]["in_animation"] == "缩放"
    assert infos[2]["loop_animation"] == "闪烁"
    assert infos[2]["out_animation"] == "翻转"
    
    # 验证duration正确添加
    assert infos[0]["in_animation_duration"] == 500000
    assert infos[0]["loop_animation_duration"] == 1000000
    assert infos[0]["out_animation_duration"] == 300000
    
    print("✓ 多种动画类型测试通过")


def test_backward_compatibility():
    """测试向后兼容性：单个动画仍然正常工作"""
    print("\n测试向后兼容性:")
    
    imgs = [
        "https://example.com/image1.jpg",
        "https://example.com/image2.jpg"
    ]
    
    timelines = [
        {"start": 0, "end": 1000000},
        {"start": 1000000, "end": 2000000}
    ]
    
    # 使用单个动画（原始用法）
    in_animation = "淡入"
    in_animation_duration = 500000
    
    infos_json = imgs_infos(
        imgs=imgs,
        timelines=timelines,
        in_animation=in_animation,
        in_animation_duration=in_animation_duration
    )
    
    infos = json.loads(infos_json)
    print(f"生成的图片信息: {infos}")
    
    # 验证向后兼容性：两个图片都使用同一个动画
    assert len(infos) == 2
    assert infos[0]["in_animation"] == "淡入"
    assert infos[1]["in_animation"] == "淡入"
    assert infos[0]["in_animation_duration"] == 500000
    assert infos[1]["in_animation_duration"] == 500000
    
    print("✓ 向后兼容性测试通过")


def test_empty_and_none_animations():
    """测试空动画和None值的处理"""
    print("\n测试空动画和None值的处理:")
    
    imgs = [
        "https://example.com/image1.jpg",
        "https://example.com/image2.jpg"
    ]
    
    timelines = [
        {"start": 0, "end": 1000000},
        {"start": 1000000, "end": 2000000}
    ]
    
    # 测试空字符串和None值
    infos_json = imgs_infos(
        imgs=imgs,
        timelines=timelines,
        in_animation="",  # 空字符串
        loop_animation=None,  # None值
        out_animation="淡出"  # 正常值
    )
    
    infos = json.loads(infos_json)
    print(f"生成的图片信息: {infos}")
    
    # 验证空动画不添加动画字段
    assert "in_animation" not in infos[0]
    assert "in_animation" not in infos[1]
    assert "loop_animation" not in infos[0]
    assert "loop_animation" not in infos[1]
    
    # 验证正常动画正常工作
    assert infos[0]["out_animation"] == "淡出"
    assert infos[1]["out_animation"] == "淡出"
    
    print("✓ 空动画和None值处理测试通过")


def run_all_tests():
    """运行所有测试"""
    print("开始测试imgs_infos多动画功能...")
    print("=" * 50)
    
    try:
        test_multiple_animations_basic()
        test_animation_extension_logic()
        test_excess_animations()
        test_multiple_animation_types()
        test_backward_compatibility()
        test_empty_and_none_animations()
        
        print("\n" + "=" * 50)
        print("🎉 所有测试通过!")
        return True
    except Exception as e:
        print(f"\n❌ 测试失败: {e}")
        import traceback
        traceback.print_exc()
        return False


if __name__ == "__main__":
    success = run_all_tests()
    sys.exit(0 if success else 1)