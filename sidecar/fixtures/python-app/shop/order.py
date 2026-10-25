from .pricing import rate


class Order:
    def total(self):
        return self._sum() * rate()

    def _sum(self):
        return 1
