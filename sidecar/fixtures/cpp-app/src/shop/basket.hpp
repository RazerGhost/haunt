#pragma once

#include "order.hpp"

namespace shop {

class Basket {
public:
    int count() const;
    int sum() const;
    virtual void print() const = 0;
};

inline int Basket::sum() const {
    return count() * 2;
}

}
