pub(crate) mod java {
    pub(crate) mod parse {
        pub(crate) const OUTER_TYPE_WITH_NESTED_TYPE: &str = r#"
            package com.example;

            public class UserService {
                private final UserRepository repository;

                public UserService(UserRepository repository) {
                    this.repository = repository;
                }

                public User findById(Long id) {
                    return null;
                }

                public static class Inner {
                    private int value;
                }
            }

            public enum Status {
                ACTIVE,
                INACTIVE
            }
        "#;

        pub(crate) const ANNOTATIONS_SEPARATE_FROM_MODIFIERS: &str = r#"
            @Entity
            public record User(
                @NotNull String name,
                @Size(max = 20) String... tags
            ) {
                @Inject
                public User {}

                @Column(name = "email")
                private final String email;

                @Deprecated
                public String email(@NotNull String fallback) {
                    return email;
                }
            }
        "#;

        pub(crate) const ANNOTATION_ELEMENT_ANNOTATIONS: &str = r#"
            public @interface Labels {
                @Deprecated
                String value();
            }
        "#;

        pub(crate) const JAVADOC_DECLARATIONS: &str = r#"
            /**
             * Service for users.
             *
             * @since 1.0
             */
            public class UserService {
                /** Repository storage. */
                private final UserRepository repository;

                /**
                 * Creates the service.
                 *
                 * @param repository storage dependency
                 */
                public UserService(UserRepository repository) {}

                /**
                 * Finds a user.
                 * Continues the description.
                 *
                 * @param id user id
                 *   continued tag text
                 * @return optional user
                 */
                public Optional<User> findById(Long id) {
                    return Optional.empty();
                }
            }

            public @interface Labels {
                /** Label value. */
                String value();
            }
        "#;

        pub(crate) const NON_JAVADOC_AND_NON_LEADING_COMMENTS: &str = r#"
            /* Ordinary block comment. */
            public class Ordinary {
                /* Ordinary field comment. */
                private int x;

                /** This documents the field, not the method. */
                private int y;

                public void run() {}
            }
        "#;

        pub(crate) const STRUCTURED_TYPE_REFERENCES: &str = r#"
            import java.util.List;
            import java.util.Map;

            public class Types extends Base<String> implements Handler<Map<String, List<Integer[]>>> {
                private List<? extends Number>[] numbers;
                private String legacy[];
                public void accept(List<? super User> users, @Valid String[] names) {}
                public String legacyReturn()[];
            }
        "#;

        pub(crate) const THROWS_TYPE_PARAMETERS_AND_NESTED_GENERICS: &str = r#"
            import java.io.IOException;
            import java.io.Serializable;
            import java.util.List;
            import java.util.Map;

            public class Types<T extends Serializable & Comparable<T>> {
                public <C extends Config & AutoCloseable> Types() throws ConfigurationException {}

                public <E extends Exception> Map.Entry<String, List<User.Id>> read()
                        throws IOException, Outer.InnerException {
                    return null;
                }
            }
        "#;

        pub(crate) const STRUCTURED_ANNOTATION_VALUES_AND_DEFAULTS: &str = r#"
            @Column(name = "email", nullable = false, roles = {"ADMIN", "USER"}, nested = @Inner(count = 2), type = String.class)
            public @interface Labels {
                @Deprecated
                String value() default "user";
                int count() default 1 + 2;
            }
        "#;
    }

    pub(crate) mod resolve {
        pub(crate) const EXPLICIT_IMPORT: &[&str] = &[
            "package com.example.model; public class User {}",
            "package com.example.service; import com.example.model.User; \
             public class Service { private User user; }",
        ];

        pub(crate) const SAME_PACKAGE_WITHOUT_IMPORT: &[&str] = &[
            "package x; public class A {}",
            "package x; public class B { A a; }",
        ];

        pub(crate) const WILDCARD_IMPORT: &[&str] = &[
            "package com.foo; public class Foo {}",
            "package other; import com.foo.*; public class Bar { Foo f; }",
        ];

        pub(crate) const NESTED_TYPE_VIA_OUTER: &[&str] = &[
            "package x; public class Outer { public static class Inner {} }",
            "package x; public class B { Outer.Inner field; }",
        ];

        pub(crate) const FULLY_QUALIFIED_INLINE_USE: &[&str] = &[
            "package com.example; public class Thing {}",
            "package other; public class B { com.example.Thing t; }",
        ];

        pub(crate) const EXTERNAL_TYPES: &[&str] = &["package a; public class A { String s; }"];

        pub(crate) const GENERIC_ARGS: &[&str] = &[
            "package a; public class User {}",
            "package b; import a.User; import java.util.List; \
             public class Holder { List<User> users; }",
        ];

        pub(crate) const ARRAY_ELEMENT: &[&str] = &[
            "package a; public class User {}",
            "package b; import a.User; public class B { User[] users; }",
        ];

        pub(crate) const WILDCARD_BOUND: &[&str] = &[
            "package a; public class Animal {}",
            "package b; import a.Animal; import java.util.List; \
             public class B { List<? extends Animal> list; }",
        ];

        pub(crate) const SAME_FILE_REFERENCE: &[&str] =
            &["package x; public class Foo { Bar b; } class Bar {}"];
    }

    pub(crate) mod output {
        pub(crate) const MARKDOWN_FULL_USER_SERVICE: &str = r#"
            package com.example;

            import java.util.Optional;

            @Service
            public class UserService<T extends AutoCloseable> {
                @Inject
                private final UserRepository repository;

                @Autowired
                public UserService(UserRepository repository) throws ConfigurationException {
                    this.repository = repository;
                }

                @Deprecated
                public <E extends Exception> Optional<User> findById(@NotNull Long id) throws E {
                    return Optional.empty();
                }

                public static class Inner {
                    private int value;
                }
            }

            public enum Status {
                ACTIVE,
                INACTIVE
            }
        "#;

        pub(crate) const MARKDOWN_MULTI_FILE_A: &str = "package a; public class A {}";
        pub(crate) const MARKDOWN_MULTI_FILE_B: &str = "package b; public class B {}";

        pub(crate) const MARKDOWN_JAVADOCS: &str = r#"
            /** Service docs. */
            public class UserService {
                /**
                 * Repository docs.
                 * @see UserRepository
                 */
                private final UserRepository repository;

                /**
                 * Finds a user.
                 *
                 * @param id user id
                 * @return optional user
                 */
                public Optional<User> findById(Long id) {
                    return Optional.empty();
                }
            }
        "#;

        pub(crate) const MARKDOWN_STANDARD_FIELD_ACCESSORS: &str = r#"
            package com.example;

            public class UserSendCodeRequest {
                private String to;
                private String channel;
                private boolean active;

                public String getTo() { return to; }
                public UserSendCodeRequest setTo(String to) { this.to = to; return this; }
                public String getChannel() { return channel; }
                public void setChannel(String channel) { this.channel = channel; }
                public boolean isActive() { return active; }
            }
        "#;

        pub(crate) const MARKDOWN_NON_MATCHING_ACCESSOR_METHODS: &str = r#"
            package com.example;

            public class UserService {
                private String name;
                private List<String> items;

                /** Name docs. */
                public String getName() { return name; }
                @Deprecated
                public void setName(String name) { this.name = name; }
                public String getOther() { return "other"; }
                public UserService addItems(List<String> items) { this.items = items; return this; }
            }
        "#;

        pub(crate) const MARKDOWN_BORING_NO_ARG_CONSTRUCTORS: &str = r#"
            package com.example;

            public class PlainDto {
                public PlainDto() {}
                public PlainDto(String name) {}
            }

            public class ConfiguredDto {
                @Inject
                public ConfiguredDto() {}
            }
        "#;

        pub(crate) const ROUND_TRIP_FOO: &str = r#"
            package com.example;
            public class Foo {
                private int x;
            }
        "#;

        pub(crate) const RESOLVED_FQN: &[&str] = super::resolve::EXPLICIT_IMPORT;

        pub(crate) const JAVADOC_USER_SERVICE: &str = r#"
            /**
             * User service.
             * @since 1.0
             */
            public class UserService {}
        "#;

        pub(crate) const TOML_EMPTY_SHAPE: &str = r#"
            package com.example;

            public class EmptyShape {
            }
        "#;

        pub(crate) const TOML_NON_EMPTY_SHAPE: &str = r#"
            package com.example;

            import java.util.List;

            @Deprecated
            public class NonEmptyShape<T> extends Base implements Runnable {
                private List<String> names;

                public void run() {
                }
            }
        "#;
    }
}
