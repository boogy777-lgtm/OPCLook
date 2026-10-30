#!/usr/bin/env python3
"""Operate the fixture's pre-existing, per-run Windows events; no OPC calls."""
import ctypes
from ctypes import wintypes
import os
import sys

kernel = ctypes.WinDLL("kernel32", use_last_error=True)
kernel.OpenEventW.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.LPCWSTR]
kernel.OpenEventW.restype = wintypes.HANDLE
kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
kernel.WaitForSingleObject.restype = wintypes.DWORD
kernel.SetEvent.argtypes = [wintypes.HANDLE]
kernel.SetEvent.restype = wintypes.BOOL
kernel.CloseHandle.argtypes = [wintypes.HANDLE]
kernel.CloseHandle.restype = wintypes.BOOL

name, action = sys.argv[1:]
handle = kernel.OpenEventW(0x00100002, False, os.environ["OPCLOOK_ISOLATION_PREFIX"] + "-" + name)
if not handle:
    raise ctypes.WinError(ctypes.get_last_error())
try:
    if action == "set":
        if not kernel.SetEvent(handle):
            raise ctypes.WinError(ctypes.get_last_error())
    elif action == "wait":
        result = kernel.WaitForSingleObject(handle, 15000)
        if result != 0:
            raise RuntimeError(f"event {name} wait failed: {result:#x}")
    elif action == "assert-unset":
        result = kernel.WaitForSingleObject(handle, 0)
        if result != 0x102:
            raise RuntimeError(f"event {name} was signaled or wait failed: {result:#x}")
    else:
        raise ValueError(action)
finally:
    kernel.CloseHandle(handle)
