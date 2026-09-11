"""Expose expected Comfy CLI failures through MCP without leaking credentials."""
import os
import re
from urllib.parse import urlsplit


def safe_detail(error):
    detail = str(error)
    # Strip all URL credentials, queries and fragments from diagnostic text.
    def endpoint(match):
        try:
            p = urlsplit(match.group(0))
            return f"{p.scheme}://{p.hostname or '[invalid-host]'}" + (f":{p.port}" if p.port else '') + p.path
        except ValueError:
            return '[redacted-url]'
    detail = re.sub(r'https?://[^\s\"\'<>]+', endpoint, detail)
    for key, value in os.environ.items():
        if value and any(word in key.upper() for word in ('KEY', 'TOKEN', 'SECRET', 'PASSWORD')):
            detail = detail.replace(value, '[redacted]')
    detail = re.sub(r'(?i)(Bearer\s+)[^\s\"\']+', r'\1[redacted]', detail)
    return detail[-1600:]


def expose_expected_errors(mcp):
    from comfy_mcp.errors import ComfyCliError
    from mcp.server.mcpserver.exceptions import ToolError, UnexpectedToolError
    original = mcp.call_tool

    async def call_tool(name, arguments, context=None):
        try:
            return await original(name, arguments, context)
        except UnexpectedToolError as error:
            cause = error.__cause__
            while isinstance(cause, UnexpectedToolError):
                cause = cause.__cause__
            if isinstance(cause, (ComfyCliError, OSError)):
                raise ToolError(f'{name} 执行失败：{safe_detail(cause)}') from None
            raise
    mcp.call_tool = call_tool
