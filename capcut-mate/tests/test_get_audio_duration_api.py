"""
测试 get_audio_duration 接口的完整性
"""
import requests
import json

def test_get_audio_duration_api():
    """测试 get_audio_duration 接口"""
    
    # 服务器地址
    base_url = "http://localhost:60000"
    api_url = f"{base_url}/openapi/v1/get_audio_duration"
    
    # 测试数据
    test_data = {
        "mp3_url": "https://www.soundjay.com/misc/sounds/bell-ringing-05.wav"
    }
    
    # 请求头
    headers = {
        "Content-Type": "application/json"
    }
    
    try:
        print("🚀 开始测试 get_audio_duration 接口...")
        print(f"📍 请求URL: {api_url}")
        print(f"📝 请求数据: {json.dumps(test_data, indent=2)}")
        
        # 发送POST请求
        response = requests.post(api_url, json=test_data, headers=headers, timeout=60)
        
        print(f"📊 响应状态码: {response.status_code}")
        print(f"📄 响应头: {dict(response.headers)}")
        
        if response.status_code == 200:
            result = response.json()
            print(f"✅ 请求成功！")
            print(f"📋 响应数据: {json.dumps(result, indent=2)}")
            
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
    success = test_get_audio_duration_api()
    if success:
        print("\n🎉 get_audio_duration 接口测试通过！")
    else:
        print("\n💥 get_audio_duration 接口测试失败！")