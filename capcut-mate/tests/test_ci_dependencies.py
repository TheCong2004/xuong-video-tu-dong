#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""
CI/CD跨平台依赖测试脚本
验证在不同环境下的依赖安装行为
"""

import sys
import subprocess
import platform

def test_platform_info():
    """显示平台信息"""
    print(f"📊平信息:")
    print(f"  系统: {platform.system()}")
    print(f"  版本: {platform.release()}")
    print(f"   Python: {sys.platform}")
    print(f"  架: {platform.machine()}")
    print()

def test_basic_sync():
    """测试基础依赖同步"""
    print("🧪测试基础依赖同步 (uv sync):")
    try:
        result = subprocess.run(['uv', 'sync'], capture_output=True, text=True, timeout=60)
        if result.returncode == 0:
            print("   ✅基础依赖同步成功")
        else:
            print("  ❌基础依赖同步失败")
            print(f"   错误信息: {result.stderr[:200]}")
    except Exception as e:
        print(f"   ❌执行失败: {e}")
    print()

def test_windows_extras():
    """测试Windows可选依赖"""
    print("🧪测试Windows可选依赖 (uv pip install -e .[windows]):")
    try:
        result = subprocess.run(['uv', 'pip', 'install', '-e', '.[windows]'], 
                             capture_output=True, text=True, timeout=60)
        if result.returncode == 0:
            print("   ✅ Windows可选依赖安装成功")
        else:
            print("  ⚠  Windows可选依赖安装可能部分成功或跳过")
            if "No candidates were found" in result.stderr:
                print("  💡这是正常的 - 在非Windows平台上会跳过Windows特定依赖")
            else:
                print(f"   错误信息: {result.stderr[:200]}")
    except Exception as e:
        print(f"   ❌执行失败: {e}")
    print()

def test_import_functionality():
    """测试功能导入"""
    print("🧪测试功能导入:")
    try:
        # 添加项目路径
        import sys
        import os
        sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))
        
        #测试基础导入
        import src.pyJianYingDraft as draft
        print(f"   ✅基础导入成功 (ISWIN: {draft.ISWIN})")
        
        #测试服务层
        from src import service
        print("   ✅ 服务层导入成功")
        
        #测试API层
        from src.router import v1_router
        print("   ✅ API层导入成功")
        
    except Exception as e:
        print(f"   ❌导入失败: {e}")
        import traceback
        traceback.print_exc()
    print()

def main():
    """主测试函数"""
    print("=" * 60)
    print("🚀 CI/CD跨平台依赖测试")
    print("=" * 60)
    
    test_platform_info()
    test_basic_sync()
    test_windows_extras()
    test_import_functionality()
    
    print("=" * 60)
    print("✅ 测试完成")
    print("=" * 60)

if __name__ == "__main__":
    main()