#include "money.hpp"

namespace shop {

Money::operator int() const {
    return cents_;
}

Money::operator bool() const {
    return cents_ != 0;
}

std::string Money::label() const {
    return "money";
}

}
