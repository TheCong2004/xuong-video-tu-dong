"""
测试 get_audio_duration 接口的编码问题修复
"""
import requests
import json

def test_chinese_metadata_audio():
    """测试包含中文元数据的音频文件"""
    
    # 服务器地址
    base_url = "http://localhost:60000"
    api_url = f"{base_url}/openapi/v1/get_audio_duration"
    
    # 测试数据 - 使用包含中文元数据的音频文件
    test_data = {
        "mp3_url": "https://assets.jcaigc.cn/test1.mp3"  # 这个文件包含中文元数据
    }
    
    # 请求头
    headers = {
        "Content-Type": "application/json"
    }
    
    try:
        print("🚀 开始测试包含中文元数据的音频文件...")
        print(f"📍 请求URL: {api_url}")
        print(f"📝 请求数据: {json.dumps(test_data, indent=2)}")
        
        # 发送POST请求
        response = requests.post(api_url, json=test_data, headers=headers, timeout=120)
        
        print(f"📊 响应状态码: {response.status_code}")
        
        if response.status_code == 200:
            result = response.json()
            print(f"✅ 编码问题修复成功！")
            print(f"📋 响应数据: {json.dumps(result, indent=2, ensure_ascii=False)}")
            
            # 验证响应格式
            if "duration" in result:
                duration = result["duration"]
                print(f"🎵 音频时长: {duration} 微秒 = {duration/1000000:.3f} 秒")
                return True
            else:
                print("❌ 响应中缺少 'duration' 字段")
                return False
        else:
            print(f"❌ 请求失败，状态码: {response.status_code}")
            print(f"📄 响应内容: {response.text}")
            return False
            
    except requests.exceptions.ConnectionError:
        print("❌ 连接失败，请确保服务器已启动 (python main.py)")
        return False
    except requests.exceptions.Timeout:
        print("❌ 请求超时")
        return False
    except Exception as e:
        print(f"❌ 测试异常: {str(e)}")
        return False

if __name__ == "__main__":
    success = test_chinese_metadata_audio()
    if success:
        print("\n🎉 中文元数据编码问题修复测试通过！")
    else:
        print("\n💥 中文元数据编码问题修复测试失败！")