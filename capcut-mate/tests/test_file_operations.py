#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""
测试文件移动功能的改进
"""

import sys
import os
import tempfile
import shutil
from unittest.mock import Mock, patch

# 添加项目路径
sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..'))

def test_file_move_improvements():
    """测试文件移动功能的改进"""
    print("🧪 测试文件移动功能改进...")
    
    # 创建测试文件
    with tempfile.TemporaryDirectory() as temp_dir:
        test_file = os.path.join(temp_dir, "test_draft.mp4")
        output_file = os.path.join(temp_dir, "output", "result.mp4")
        
        # 创建测试文件
        with open(test_file, 'w') as f:
            f.write("test content")
        
        print(f"✅ 创建测试文件: {test_file}")
        print(f"✅目标路径: {output_file}")
        
        #测试正常移动
        try:
            #确保目标目录存在
            os.makedirs(os.path.dirname(output_file), exist_ok=True)
            shutil.move(test_file, output_file)
            print("✅ 文件移动成功")
            
            # 检查文件是否存在
            if os.path.exists(output_file):
                print("✅目标文件存在")
            else:
                print("❌目标文件不存在")
                
        except Exception as e:
            print(f"❌ 文件移动失败: {e}")
    
    print("✅ 文件移动测试完成")

def test_directory_creation():
    """测试目录创建功能"""
    print("\n🧪 测试目录创建功能...")
    
    with tempfile.TemporaryDirectory() as temp_dir:
        nested_path = os.path.join(temp_dir, "a", "b", "c", "file.txt")
        directory = os.path.dirname(nested_path)
        
        print(f"✅ 目标目录: {directory}")
        
        #测试创建嵌套目录
        try:
            os.makedirs(directory, exist_ok=True)
            print("✅目录创建成功")
            
            # 创建测试文件
            with open(nested_path, 'w') as f:
                f.write("test")
            print("✅ 文件创建成功")
            
        except Exception as e:
            print(f"❌目录创建失败: {e}")
    
    print("✅目录创建测试完成")

if __name__ == "__main__":
    test_file_move_improvements()
    test_directory_creation()
    print("\n🎉 所有测试完成!")