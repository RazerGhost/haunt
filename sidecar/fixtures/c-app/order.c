#include "order.h"

static int rate(void) {
    return 2;
}

struct order order_new(int count) {
    struct order o = {count};
    return o;
}

int order_total(const struct order *o) {
    return o->count * rate();
}
