"""WireGuard 操作まわりで共通利用する例外。"""


class WireGuardCommandError(RuntimeError):
    def __init__(
        self,
        message: str,
        *,
        returncode: int | None = None,
        stderr: str | None = None,
        response: dict | None = None,
    ) -> None:
        super().__init__(message)
        self.returncode = returncode
        self.stderr = stderr
        self.response = response
