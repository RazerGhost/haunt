#ifndef ORDER_H
#define ORDER_H

struct order {
    int count;
};

struct order order_new(int count);
int order_total(const struct order *o);

#endif
