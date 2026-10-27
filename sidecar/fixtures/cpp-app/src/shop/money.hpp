#pragma once

#include <string>

namespace shop {

class Money {
public:
    explicit Money(int cents) : cents_(cents) {}
    operator int() const;
    explicit operator bool() const;
    operator std::string() const { return label(); }

private:
    std::string label() const;
    int cents_;
};

}
