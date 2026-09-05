from .config import models_path


class Engine:
    def __init__(self):
        from kokoro_onnx import Kokoro

        models = models_path()
        self._kokoro = Kokoro(
            str(models / "kokoro-v1.0.fp16.onnx"), str(models / "voices-v1.0.bin")
        )

    def synthesize(self, text: str, voice: str, speed: float, lang: str):
        samples, rate = self._kokoro.create(
            text, voice=voice, speed=speed, lang=lang
        )
        return samples, rate


class FakeEngine:
    def synthesize(self, text: str, voice: str, speed: float, lang: str):
        return [0.0] * 2400, 24000
