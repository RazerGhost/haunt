#pragma once

namespace shop {

class Order {
public:
    static Order make(int count);
    int total() const;

private:
    explicit Order(int count) : count_(count) {}
    int count_;
};

}
