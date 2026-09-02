from .config import models_path


class Engine:
    def __init__(self, cfg: dict):
        from kokoro_onnx import Kokoro

        models = models_path()
        self._kokoro = Kokoro(
            str(models / "kokoro-v1.0.fp16.onnx"), str(models / "voices-v1.0.bin")
        )
        self._cfg = cfg

    def synthesize(self, text: str, voice: str | None = None):
        samples, rate = self._kokoro.create(
            text,
            voice=voice or self._cfg["voice"],
            speed=self._cfg["speed"],
            lang=self._cfg["lang"],
        )
        return samples, rate
