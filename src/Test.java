package com.example.demo;

import java.util.Optional;
import java.util.List;

/**
 * This is a test class.
 */
public final class UserService implements Auditable {

    private final UserRepository repository;
    protected static int count;
    public static final int MAX_SIZE = 100;

    public UserService(UserRepository repository) {
        this.repository = repository;
    }

    public Optional<User> findById(Long id) {
        return Optional.empty();
    }

    public void process(String name, int age) {
        System.out.println(name);
    }

    public static class Inner {
        private int value;

        public Inner(int value) {
            this.value = value;
        }

        public int getValue() {
            return value;
        }
    }
}

public record UserDto(Long id, String name) {}

public enum Status {
    ACTIVE,
    INACTIVE
}

@interface Audited {
    String value();
}
