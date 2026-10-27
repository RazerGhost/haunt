#include <stdio.h>
#include "order.h"

int main(void) {
    struct order o = order_new(2);
    printf("%d\n", order_total(&o));
    return 0;
}
